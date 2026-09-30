//! A bounded logical paragraph. Inline boundaries do not introduce line breaks.
//! Normalization, UAX #14 opportunities and UAX #9 levels precede placement.
use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use unicode_bidi::{BidiInfo, Level};
use unicode_linebreak::linebreaks;
use unicode_segmentation::UnicodeSegmentation;

use crate::dom::NodeId;
use crate::style::{ComputedStyle, Direction, WhiteSpace};
use crate::text::{self, ResolvedRun};

pub(crate) const MAX_PARAGRAPH_CHARS: usize = 200_000;

#[derive(Clone)]
struct Origin {
    range: Range<usize>,
    source: Range<usize>,
    node: NodeId,
    active: Arc<[NodeId]>,
    atomic: bool,
    control: bool,
}

#[derive(Clone)]
pub(crate) enum PieceKind {
    Text(ResolvedRun),
    Atomic(NodeId),
    Edge,
    Tab,
}

#[derive(Clone)]
pub(crate) struct Piece {
    pub kind: PieceKind,
    pub range: Range<usize>,
    pub source: Range<usize>,
    pub node: NodeId,
    pub active: Arc<[NodeId]>,
    pub level: Level,
    pub width: f32,
    pub collapsible: bool,
    pub edge_end: Option<bool>,
}

pub(crate) struct Group {
    pub pieces: Vec<Piece>,
    pub forced: bool,
    pub wrap: bool,
    pub glue: bool,
}

struct Edge {
    offset: usize,
    node: NodeId,
    active: Arc<[NodeId]>,
    width: (f32, f32),
    end: bool,
}

pub(crate) struct Paragraph {
    text: String,
    origins: Vec<Origin>,
    edges: Vec<Edge>,
    pending: Option<Origin>,
    pub truncated: bool,
    budget: Rc<Cell<usize>>,
    collapsed_tail: bool,
}

impl Paragraph {
    pub fn new(budget: Rc<Cell<usize>>) -> Self {
        Self {
            text: String::new(),
            origins: Vec::new(),
            edges: Vec::new(),
            pending: None,
            truncated: false,
            budget,
            collapsed_tail: false,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.edges.is_empty()
    }

    fn append(&mut self, c: char, mut origin: Origin) {
        if self.origins.len() >= MAX_PARAGRAPH_CHARS || self.budget.get() == 0 {
            self.truncated = true;
            return;
        }
        self.budget.set(self.budget.get() - 1);
        origin.range = self.text.len()..self.text.len() + c.len_utf8();
        self.text.push(c);
        self.origins.push(origin);
    }

    fn flush_space(&mut self) {
        if let Some(origin) = self.pending.take() {
            self.append(' ', origin);
            self.collapsed_tail = true;
        }
    }

    pub fn text(&mut self, source: &str, node: NodeId, active: &[NodeId], style: &ComputedStyle) {
        let active: Arc<[NodeId]> = Arc::from(active);
        for (offset, c) in source.char_indices() {
            if self.truncated {
                break;
            }
            let origin = Origin {
                range: 0..0,
                source: offset..offset + c.len_utf8(),
                node,
                active: active.clone(),
                atomic: false,
                control: false,
            };
            let preserved = matches!(style.white_space, WhiteSpace::Pre | WhiteSpace::PreWrap);
            if c == '\n' && (preserved || style.white_space == WhiteSpace::PreLine) {
                self.pending = None;
                self.append(c, origin);
                self.collapsed_tail = false;
            } else if c.is_ascii_whitespace() && !preserved {
                // The first collapsed character owns the cross-element space.
                if self.pending.is_none() && !self.collapsed_tail {
                    self.pending = Some(origin);
                }
            } else {
                self.flush_space();
                self.append(c, origin);
                self.collapsed_tail = false;
            }
        }
    }

    pub fn edge(&mut self, node: NodeId, active: &[NodeId], width: (f32, f32), end: bool) {
        self.flush_space();
        if self.budget.get() == 0 {
            self.truncated = true;
            return;
        }
        self.budget.set(self.budget.get() - 1);
        self.edges.push(Edge {
            offset: self.text.len(),
            node,
            active: Arc::from(active),
            width,
            end,
        });
    }

    pub fn special(
        &mut self,
        c: char,
        node: NodeId,
        active: &[NodeId],
        atomic: bool,
        control: bool,
    ) {
        if c == '\n' {
            self.pending = None;
        } else {
            self.flush_space();
        }
        self.append(
            c,
            Origin {
                range: 0..0,
                source: 0..0,
                node,
                active: Arc::from(active),
                atomic,
                control,
            },
        );
        if !control {
            self.collapsed_tail = false;
        }
    }

    pub fn prepare(mut self, styles: &[ComputedStyle], direction: Direction) -> Vec<Group> {
        // Collapsible trailing paragraph whitespace is discarded, preserved text isn't.
        self.pending = None;
        if self.truncated
            && let Some((offset, _)) = self.text.grapheme_indices(true).next_back()
        {
            // The work budget can expire in the middle of a source cluster,
            // including one continued by a later DOM text node. Discard the
            // final normalized cluster conservatively rather than painting a
            // prefix of it. The already spent work budget is not refunded.
            self.text.truncate(offset);
            self.origins.retain(|origin| origin.range.end <= offset);
            self.edges.retain(|edge| edge.offset <= offset);
        }
        let base = if direction == Direction::Rtl {
            Level::rtl()
        } else {
            Level::ltr()
        };
        let bidi = BidiInfo::new(&self.text, Some(base));
        // Each edge takes the embedding level of content inside its own
        // inline box, rather than the preceding outside character or the
        // opening bidi control. Precomputed nearest non-control origins keep
        // empty/nested bidi spans from turning this lookup into repeated scans.
        let mut preceding = vec![None; self.origins.len() + 1];
        let mut following = vec![None; self.origins.len() + 1];
        for (index, origin) in self.origins.iter().enumerate() {
            preceding[index + 1] = if origin.control {
                preceding[index]
            } else {
                Some(index)
            };
        }
        for (index, origin) in self.origins.iter().enumerate().rev() {
            following[index] = if origin.control {
                following[index + 1]
            } else {
                Some(index)
            };
        }
        let edge_levels: Vec<_> = self
            .edges
            .iter()
            .map(|edge| {
                let at = self
                    .origins
                    .partition_point(|origin| origin.range.start < edge.offset);
                let inside = following[at]
                    .filter(|&index| self.origins[index].active.contains(&edge.node))
                    .or_else(|| {
                        preceding[at]
                            .filter(|&index| self.origins[index].active.contains(&edge.node))
                    });
                inside
                    .and_then(|index| bidi.levels.get(self.origins[index].range.start).copied())
                    .unwrap_or(base)
            })
            .collect();
        let mut boundaries: Vec<_> = self.text.grapheme_indices(true).map(|(i, _)| i).collect();
        boundaries.push(self.text.len());
        let breaks: Vec<_> = linebreaks(&self.text)
            .filter_map(|(offset, _)| boundaries.binary_search(&offset).ok().map(|_| offset))
            .collect();
        let mut groups = Vec::new();
        let mut start = 0;
        let mut edge_index = 0;
        let mut origin_index = 0;
        for end in breaks.into_iter().chain(std::iter::once(self.text.len())) {
            if end == start && end != self.text.len() {
                continue;
            }
            if end == start && edge_index == self.edges.len() {
                continue;
            }
            let mut group = Group {
                pieces: Vec::new(),
                forced: false,
                wrap: true,
                glue: false,
            };
            while origin_index < self.origins.len() && self.origins[origin_index].range.start < end
            {
                let first = &self.origins[origin_index];
                let offset = first.range.start;
                while edge_index < self.edges.len() && self.edges[edge_index].offset <= offset {
                    group
                        .pieces
                        .push(edge_piece(&self.edges[edge_index], edge_levels[edge_index]));
                    edge_index += 1;
                }
                let c = self.text[offset..].chars().next().unwrap();
                let style = &styles[first.node];
                group.wrap &= !matches!(style.white_space, WhiteSpace::NoWrap | WhiteSpace::Pre);
                group.glue |= matches!(c, '\u{a0}' | '\u{202f}' | '\u{2060}');
                if c == '\n' {
                    group.forced = true;
                    origin_index += 1;
                    continue;
                }
                if first.control {
                    origin_index += 1;
                    continue;
                }
                let level = bidi.levels.get(offset).copied().unwrap_or(base);
                if first.atomic || c == '\t' {
                    group.pieces.push(Piece {
                        kind: if first.atomic {
                            PieceKind::Atomic(first.node)
                        } else {
                            PieceKind::Tab
                        },
                        range: first.range.clone(),
                        source: first.source.clone(),
                        node: first.node,
                        active: first.active.clone(),
                        level,
                        width: 0.0,
                        collapsible: false,
                        edge_end: None,
                    });
                    origin_index += 1;
                    continue;
                }
                let whitespace = c == ' ';
                let collapsed = whitespace
                    && self.text[offset..].graphemes(true).next() == Some(" ")
                    && !matches!(style.white_space, WhiteSpace::Pre | WhiteSpace::PreWrap);
                let mut limit = origin_index + 1;
                // Shape complete styled directional words, including all combining characters.
                // A style change is a shaping boundary; no line opportunity is added there.
                while limit < self.origins.len() {
                    let next = &self.origins[limit];
                    let grapheme_start = boundaries.binary_search(&next.range.start).is_ok();
                    if next.range.start >= end
                        || (grapheme_start
                            && (next.node != first.node
                                || next.active != first.active
                                || bidi.levels[next.range.start] != level))
                        || next.atomic
                        || next.control
                        || self
                            .edges
                            .get(edge_index)
                            .is_some_and(|edge| edge.offset <= next.range.start && grapheme_start)
                    {
                        break;
                    }
                    let n = self.text[next.range.start..].chars().next().unwrap();
                    if matches!(n, '\n' | '\t') || (grapheme_start && whitespace != (n == ' ')) {
                        break;
                    }
                    limit += 1;
                }
                let range = first.range.start..self.origins[limit - 1].range.end;
                let value = &self.text[range.clone()];
                group.glue |= value
                    .chars()
                    .any(|c| matches!(c, '\u{a0}' | '\u{202f}' | '\u{2060}'));
                let compatible = |origin: &Origin| {
                    let other = &styles[origin.node];
                    other.font_size == style.font_size
                        && other.bold == style.bold
                        && bidi.levels[origin.range.start] == level
                        && !origin.atomic
                        && !origin.control
                };
                let decorated = |offset| {
                    let first_edge = self.edges.partition_point(|edge| edge.offset < offset);
                    self.edges[first_edge..]
                        .iter()
                        .take_while(|edge| edge.offset == offset)
                        .any(|edge| edge.width.0 != 0.0 || edge.width.1 != 0.0)
                };
                let pre = if origin_index > 0
                    && compatible(&self.origins[origin_index - 1])
                    && !decorated(range.start)
                {
                    &self.text[..range.start]
                } else {
                    ""
                };
                let post =
                    if self.origins.get(limit).is_some_and(compatible) && !decorated(range.end) {
                        &self.text[range.end..]
                    } else {
                        ""
                    };
                let mut local = 0;
                for run in text::resolve_context(
                    value,
                    style.font_size,
                    style.bold,
                    level.is_rtl(),
                    pre,
                    post,
                ) {
                    let run_end = local + run.content.len();
                    let mapped_start = range.start + local;
                    let mapped_end = range.start + run_end;
                    let source_origins = &self.origins[origin_index..limit];
                    let a = source_origins.partition_point(|o| o.range.end <= mapped_start);
                    let b = source_origins.partition_point(|o| o.range.start < mapped_end);
                    // A cluster crossing an element boundary belongs
                    // to its first character's style/node. Its primary source
                    // mapping stays inside that node even when later marks are
                    // supplied by a different DOM node. Decoration edges inside
                    // the cluster are emitted after its whole ink instead of
                    // splitting shaping or introducing an emergency break.
                    let source_start = source_origins[a..b]
                        .iter()
                        .find(|o| o.node == first.node)
                        .map_or(first.source.start, |o| o.source.start);
                    let source_end = source_origins[a..b]
                        .iter()
                        .rev()
                        .find(|o| o.node == first.node)
                        .map_or(first.source.end, |o| o.source.end);
                    group.pieces.push(Piece {
                        width: run.advance,
                        kind: PieceKind::Text(run),
                        range: range.start + local..range.start + run_end,
                        source: source_start..source_end,
                        node: first.node,
                        active: first.active.clone(),
                        level,
                        collapsible: collapsed,
                        edge_end: None,
                    });
                    local = run_end;
                }
                origin_index = limit;
            }
            while edge_index < self.edges.len() && self.edges[edge_index].offset <= end {
                group
                    .pieces
                    .push(edge_piece(&self.edges[edge_index], edge_levels[edge_index]));
                edge_index += 1;
            }
            if !group.pieces.is_empty() || group.forced {
                groups.push(group);
            }
            start = end;
        }
        groups
    }
}

fn edge_piece(edge: &Edge, level: Level) -> Piece {
    Piece {
        kind: PieceKind::Edge,
        range: edge.offset..edge.offset,
        source: 0..0,
        node: edge.node,
        active: edge.active.clone(),
        // Physical left/right decorations follow the inline box's direction:
        // logical start is on the right for an RTL box.
        width: if edge.end == level.is_rtl() {
            edge.width.0
        } else {
            edge.width.1
        },
        level,
        collapsible: false,
        edge_end: Some(edge.end),
    }
}
