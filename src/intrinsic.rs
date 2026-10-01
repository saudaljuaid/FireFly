//! Pure intrinsic contributions using the same paragraph normalization and shaper
//! as final layout. The cache lives for one immutable document/style/image pass.
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::dom::{Document, NodeId, NodeKind};
use crate::layout::ImageSource;
use crate::paragraph::{Paragraph, PieceKind};
use crate::sizing::{
    AvailableSize, IntrinsicSizes, MAX_SIZE, constrain, content_size, resolve_content_size,
};
use crate::style::{
    BorderStyle, ComputedStyle, Display, Length, OverflowWrap, Position, UnicodeBidi, is_visible,
};
use crate::text;

pub const MAX_INTRINSIC_WORK: usize = 400_000;

pub struct IntrinsicCache<'a> {
    document: &'a Document,
    styles: &'a [ComputedStyle],
    images: &'a [Option<ImageSource>],
    cache: RefCell<Vec<Option<IntrinsicSizes>>>,
    budget: Rc<Cell<usize>>,
    numeric_budget: Cell<usize>,
    formatting: RefCell<Vec<Option<Vec<NodeId>>>>,
    anonymous: RefCell<HashMap<NodeId, Vec<NodeId>>>,
    pub truncated: Cell<bool>,
}

impl<'a> IntrinsicCache<'a> {
    pub fn new(
        document: &'a Document,
        styles: &'a [ComputedStyle],
        images: &'a [Option<ImageSource>],
    ) -> Self {
        Self {
            document,
            styles,
            images,
            cache: RefCell::new(vec![None; document.nodes.len()]),
            budget: Rc::new(Cell::new(MAX_INTRINSIC_WORK)),
            numeric_budget: Cell::new(8_000_000),
            truncated: Cell::new(false),
            formatting: RefCell::new(vec![None; document.nodes.len()]),
            anonymous: RefCell::new(HashMap::new()),
        }
    }
    pub fn content(&self, id: NodeId) -> IntrinsicSizes {
        self.content_at(id, 0)
    }

    pub fn anonymous_nodes(&self, id: NodeId) -> Vec<NodeId> {
        self.anonymous
            .borrow()
            .get(&id)
            .cloned()
            .unwrap_or_else(|| vec![id])
    }

    /// Stable order-modified items; contiguous text across comments is one
    /// anonymous item. No DOM nodes or computed styles are rewritten.
    pub fn formatting_nodes(&self, id: NodeId) -> Vec<NodeId> {
        if let Some(nodes) = self.formatting.borrow()[id].as_ref() {
            return nodes.clone();
        }
        let mut nodes = Vec::new();
        let mut pending = Vec::new();
        let flush = |pending: &mut Vec<NodeId>, nodes: &mut Vec<NodeId>| {
            if pending.iter().any(|&node| matches!(&self.document.nodes[node].kind, NodeKind::Text(value) if value.chars().any(|c|!c.is_ascii_whitespace()))) {
                let first = pending[0];
                if nodes.len() < 4096 {
                    nodes.push(first);
                    if pending.len() > 1 { self.anonymous.borrow_mut().insert(first, pending.clone()); self.cache.borrow_mut()[first] = None; }
                } else { self.truncated.set(true); }
            }
            pending.clear();
        };
        for &child in &self.document.nodes[id].children {
            if self.numeric_budget.get() == 0 {
                self.truncated.set(true);
                break;
            }
            self.numeric_budget.set(self.numeric_budget.get() - 1);
            if !is_visible(self.document, self.styles, child)
                || self.styles[child].position == Position::Absolute
            {
                continue;
            }
            match &self.document.nodes[child].kind {
                NodeKind::Text(_) => {
                    if pending.len() >= 4096 {
                        self.truncated.set(true);
                        break;
                    }
                    pending.push(child);
                }
                NodeKind::Element(_) => {
                    flush(&mut pending, &mut nodes);
                    if nodes.len() < 4096 {
                        nodes.push(child);
                    } else {
                        self.truncated.set(true);
                        break;
                    }
                }
                _ => {}
            }
        }
        flush(&mut pending, &mut nodes);
        nodes.sort_by_key(|&node| self.styles[node].order);
        self.formatting.borrow_mut()[id] = Some(nodes.clone());
        nodes
    }

    pub fn contribution(&self, id: NodeId) -> IntrinsicSizes {
        let intrinsic = self.content(id);
        let style = &self.styles[id];
        let inset = self.horizontal_inset(id);
        let margin = style.margin.left.resolve_indefinite(None).unwrap_or(0.0)
            + style.margin.right.resolve_indefinite(None).unwrap_or(0.0);
        let resolve = |value: Option<Length>| {
            value.and_then(|value| {
                resolve_content_size(
                    intrinsic,
                    value,
                    AvailableSize::Indefinite,
                    inset,
                    style.box_sizing,
                )
            })
        };
        let preferred = resolve(style.width);
        let minimum = resolve(style.min_width);
        let maximum = resolve(style.max_width);
        IntrinsicSizes::new(
            constrain(preferred.unwrap_or(intrinsic.min_content), minimum, maximum)
                + inset
                + margin,
            constrain(preferred.unwrap_or(intrinsic.max_content), minimum, maximum)
                + inset
                + margin,
        )
    }

    fn horizontal_inset(&self, id: NodeId) -> f32 {
        let style = &self.styles[id];
        let resolve = |value: Length| value.resolve_indefinite(None).unwrap_or(0.0).max(0.0);
        resolve(style.padding.left)
            + resolve(style.padding.right)
            + if style.border_style == BorderStyle::None {
                0.0
            } else {
                resolve(style.border_width.left) + resolve(style.border_width.right)
            }
    }

    fn content_at(&self, id: NodeId, depth: usize) -> IntrinsicSizes {
        if let Some(value) = self.cache.borrow()[id] {
            return value;
        }
        if depth > 256 || self.budget.get() == 0 {
            self.truncated.set(true);
            self.cache.borrow_mut()[id] = Some(IntrinsicSizes::default());
            return IntrinsicSizes::default();
        }
        self.budget.set(self.budget.get() - 1);
        if !is_visible(self.document, self.styles, id) {
            return IntrinsicSizes::default();
        }
        let style = &self.styles[id];
        let is_image = self.document.element(id).is_some_and(|e| e.tag == "img");
        let result = if is_image {
            let dimension = |name| {
                self.document
                    .element(id)
                    .and_then(|e| e.attribute(name))
                    .and_then(|v| v.trim().parse::<f32>().ok())
                    .filter(|v| v.is_finite() && (0.0..=16_384.0).contains(v))
            };
            let image = self.images.get(id).and_then(|v| v.as_ref());
            let vertical_inset = style.padding.top.resolve_indefinite(None).unwrap_or(0.0)
                + style.padding.bottom.resolve_indefinite(None).unwrap_or(0.0)
                + if style.border_style == BorderStyle::None {
                    0.0
                } else {
                    style
                        .border_width
                        .top
                        .resolve_indefinite(None)
                        .unwrap_or(0.0)
                        + style
                            .border_width
                            .bottom
                            .resolve_indefinite(None)
                            .unwrap_or(0.0)
                };
            let height = style
                .height
                .and_then(|v| v.resolve_indefinite(None))
                .map(|v| content_size(v, vertical_inset, style.box_sizing))
                .or_else(|| {
                    if style.height_declared {
                        None
                    } else {
                        dimension("height")
                    }
                });
            let height_constraint = |value: Option<Length>| {
                value
                    .and_then(|value| value.resolve_indefinite(None))
                    .map(|value| content_size(value, vertical_inset, style.box_sizing))
            };
            let height = height.map(|height| {
                constrain(
                    height,
                    height_constraint(style.min_height),
                    height_constraint(style.max_height),
                )
            });
            let width = if style.width_declared {
                None
            } else {
                dimension("width")
            }
            .or_else(|| {
                image.map(|image| {
                    let horizontal = self.horizontal_inset(id);
                    let width_constraint = |value: Option<Length>| {
                        value
                            .and_then(|value| value.resolve_indefinite(None))
                            .map(|value| content_size(value, horizontal, style.box_sizing))
                    };
                    crate::sizing::replaced_size(
                        (image.width, image.height),
                        (
                            style
                                .width
                                .and_then(|value| value.resolve_indefinite(None))
                                .map(|value| content_size(value, horizontal, style.box_sizing)),
                            height,
                        ),
                        (
                            width_constraint(style.min_width),
                            height_constraint(style.min_height),
                        ),
                        (
                            width_constraint(style.max_width),
                            height_constraint(style.max_height),
                        ),
                    )
                    .0
                })
            })
            .unwrap_or_else(|| {
                let mut p = Paragraph::new(self.budget.clone());
                p.text(
                    self.document
                        .element(id)
                        .and_then(|e| e.attribute("alt"))
                        .unwrap_or(""),
                    id,
                    &[],
                    style,
                );
                self.paragraph(p, id, depth).max_content
            });
            IntrinsicSizes::new(width, width)
        } else if style.display.is_grid() {
            let children = self.formatting_nodes(id);
            let inputs: Vec<_> = children
                .iter()
                .map(|&child| crate::grid::PlacementInput {
                    row: self.styles[child].grid_row,
                    column: self.styles[child].grid_column,
                })
                .collect();
            let placed = crate::grid::place_items_with_budget(
                &inputs,
                style.grid_template_rows.len(),
                style.grid_template_columns.len(),
                style.grid_auto_flow,
                self.numeric_budget.get(),
            );
            self.truncated.set(self.truncated.get() || placed.truncated);
            self.numeric_budget
                .set(self.numeric_budget.get().saturating_sub(placed.work));
            let tracks: Vec<_> = (0..placed.columns)
                .map(|i| {
                    style
                        .grid_template_columns
                        .get(i)
                        .copied()
                        .unwrap_or_else(|| {
                            if style.grid_auto_columns.is_empty() {
                                crate::grid::Track::auto()
                            } else {
                                style.grid_auto_columns[(i - style.grid_template_columns.len())
                                    % style.grid_auto_columns.len()]
                            }
                        })
                })
                .collect();
            let mut contributions = Vec::new();
            for (i, area) in placed.areas.iter().enumerate() {
                if let Some(area) = area {
                    let work = area.column_span * 3 + 1;
                    if work > self.numeric_budget.get() {
                        self.truncated.set(true);
                        self.numeric_budget.set(0);
                        break;
                    }
                    self.numeric_budget.set(self.numeric_budget.get() - work);
                    let child = children[i];
                    self.content_at(child, depth + 1);
                    let sizes = self.contribution(child);
                    let cs = &self.styles[child];
                    let inset = self.horizontal_inset(child);
                    let margin = cs.margin.left.resolve_indefinite(None).unwrap_or(0.0)
                        + cs.margin.right.resolve_indefinite(None).unwrap_or(0.0);
                    let spanned = &tracks[area.column..area.column + area.column_span];
                    let resolve = |value: Option<Length>| {
                        value.and_then(|value| {
                            resolve_content_size(
                                self.content(child),
                                value,
                                AvailableSize::Indefinite,
                                inset,
                                cs.box_sizing,
                            )
                        })
                    };
                    let minimum = crate::grid::minimum_contribution(
                        spanned,
                        AvailableSize::Indefinite,
                        style.column_gap.resolve_indefinite(None).unwrap_or(0.0),
                        crate::grid::MinimumContribution {
                            content_min: self.content(child).min_content,
                            preferred: resolve(cs.width),
                            min: resolve(cs.min_width),
                            max: resolve(cs.max_width),
                            inset,
                            margins: margin,
                            preferred_outer: sizes.min_content,
                            overflow_hidden: cs.overflow_hidden,
                        },
                    );
                    contributions.push(crate::grid::Contribution {
                        start: area.column,
                        span: area.column_span,
                        min_content: sizes.min_content,
                        max_content: sizes.max_content,
                        minimum,
                    });
                }
            }
            let gap = style.column_gap.resolve_indefinite(None).unwrap_or(0.0);
            if self.numeric_budget.get() == 0 {
                self.truncated.set(true);
                IntrinsicSizes::default()
            } else {
                let min = crate::grid::size_tracks_with_budget(
                    &tracks,
                    AvailableSize::MinContent,
                    gap,
                    &contributions,
                    false,
                    self.numeric_budget.get(),
                );
                self.numeric_budget
                    .set(self.numeric_budget.get().saturating_sub(min.work));
                let max = crate::grid::size_tracks_with_budget(
                    &tracks,
                    AvailableSize::MaxContent,
                    gap,
                    &contributions,
                    false,
                    self.numeric_budget.get(),
                );
                self.numeric_budget
                    .set(self.numeric_budget.get().saturating_sub(max.work));
                self.truncated
                    .set(self.truncated.get() || min.truncated || max.truncated);
                IntrinsicSizes::new(min.extent, max.extent)
            }
        } else if matches!(style.display, Display::Flex | Display::InlineFlex) {
            let row = matches!(
                style.flex_direction,
                crate::flex::FlexDirection::Row | crate::flex::FlexDirection::RowReverse
            );
            let items = self.formatting_nodes(id);
            let gap = style.column_gap.resolve_indefinite(None).unwrap_or(0.0);
            let mut min: f32 = 0.0;
            let mut max: f32 = 0.0;
            for &child in &items {
                self.content_at(child, depth + 1);
                let mut sizes = self.contribution(child);
                if row {
                    // Intrinsic flex widths conservatively accommodate a definite
                    // basis and the item's intrinsic content; flexible fractions
                    // are resolved only during final layout at available width.
                    let cs = &self.styles[child];
                    if !matches!(cs.flex_basis, Length::Auto)
                        && let Some(basis) = resolve_content_size(
                            self.content(child),
                            cs.flex_basis,
                            AvailableSize::Indefinite,
                            self.horizontal_inset(child),
                            cs.box_sizing,
                        )
                    {
                        let margin = cs.margin.left.resolve_indefinite(None).unwrap_or(0.0)
                            + cs.margin.right.resolve_indefinite(None).unwrap_or(0.0);
                        let outer_basis = basis + self.horizontal_inset(child) + margin;
                        sizes = IntrinsicSizes::new(
                            sizes.min_content.max(outer_basis),
                            sizes.max_content.max(outer_basis),
                        );
                    }
                    if style.flex_wrap == crate::flex::FlexWrap::NoWrap {
                        min += sizes.min_content;
                    } else {
                        min = min.max(sizes.min_content);
                    }
                    max += sizes.max_content;
                } else {
                    min = min.max(sizes.min_content);
                    max = max.max(sizes.max_content);
                }
            }
            if row {
                let gaps = gap * items.len().saturating_sub(1) as f32;
                max += gaps;
                if style.flex_wrap == crate::flex::FlexWrap::NoWrap {
                    min += gaps;
                }
            }
            IntrinsicSizes::new(min, max)
        } else {
            let mut p = Paragraph::new(self.budget.clone());
            let mut active = Vec::new();
            let mut min: f32 = 0.0;
            let mut max: f32 = 0.0;
            if matches!(self.document.nodes[id].kind, NodeKind::Text(_)) {
                for node in self.anonymous_nodes(id) {
                    self.inline(node, &mut p, &mut active, depth + 1);
                }
            } else {
                for &child in &self.document.nodes[id].children {
                    if !is_visible(self.document, self.styles, child)
                        || self.styles[child].position == Position::Absolute
                    {
                        continue;
                    }
                    if self.styles[child].display.is_block_level() {
                        let done = self.paragraph(p, id, depth);
                        min = min.max(done.min_content);
                        max = max.max(done.max_content);
                        p = Paragraph::new(self.budget.clone());
                        self.content_at(child, depth + 1);
                        let sizes = self.contribution(child);
                        min = min.max(sizes.min_content);
                        max = max.max(sizes.max_content);
                    } else {
                        self.inline(child, &mut p, &mut active, depth + 1);
                    }
                }
            }
            let done = self.paragraph(p, id, depth);
            IntrinsicSizes::new(min.max(done.min_content), max.max(done.max_content))
        };
        self.cache.borrow_mut()[id] = Some(result);
        result
    }

    fn inline(&self, id: NodeId, p: &mut Paragraph, active: &mut Vec<NodeId>, depth: usize) {
        if depth > 256 || self.budget.get() == 0 {
            self.truncated.set(true);
            return;
        }
        if !is_visible(self.document, self.styles, id)
            || self.styles[id].position == Position::Absolute
        {
            return;
        }
        let style = &self.styles[id];
        match &self.document.nodes[id].kind {
            NodeKind::Text(value) => p.text(value, id, active, style),
            NodeKind::Element(e) if e.tag == "br" => p.special('\n', id, active, false, false),
            NodeKind::Element(e) if e.tag == "img" || style.display.is_atomic() => {
                p.special('\u{fffc}', id, active, true, false)
            }
            NodeKind::Element(_) => {
                active.push(id);
                let open = match style.unicode_bidi {
                    UnicodeBidi::Embed => {
                        Some(if style.direction == crate::style::Direction::Rtl {
                            '\u{202b}'
                        } else {
                            '\u{202a}'
                        })
                    }
                    UnicodeBidi::Isolate => {
                        Some(if style.direction == crate::style::Direction::Rtl {
                            '\u{2067}'
                        } else {
                            '\u{2066}'
                        })
                    }
                    _ => None,
                };
                if let Some(c) = open {
                    p.special(c, id, active, false, true);
                }
                let side = |padding: Length, border: Length, margin: Length| {
                    padding.resolve_indefinite(None).unwrap_or(0.0).max(0.0)
                        + if style.border_style == BorderStyle::None {
                            0.0
                        } else {
                            border.resolve_indefinite(None).unwrap_or(0.0).max(0.0)
                        }
                        + margin.resolve_indefinite(None).unwrap_or(0.0)
                };
                let widths = (
                    side(
                        style.padding.left,
                        style.border_width.left,
                        style.margin.left,
                    ),
                    side(
                        style.padding.right,
                        style.border_width.right,
                        style.margin.right,
                    ),
                );
                p.edge(id, active, widths, false);
                for &child in &self.document.nodes[id].children {
                    self.inline(child, p, active, depth + 1);
                }
                p.edge(id, active, widths, true);
                if open.is_some() {
                    p.special(
                        if style.unicode_bidi == UnicodeBidi::Embed {
                            '\u{202c}'
                        } else {
                            '\u{2069}'
                        },
                        id,
                        active,
                        false,
                        true,
                    );
                }
                active.pop();
            }
            _ => {}
        }
    }

    fn paragraph(&self, p: Paragraph, parent: NodeId, depth: usize) -> IntrinsicSizes {
        self.truncated.set(self.truncated.get() || p.truncated);
        let groups = p.prepare(self.styles, self.styles[parent].direction);
        let mut line: f32 = 0.0;
        let mut minimum: f32 = 0.0;
        let mut maximum: f32 = 0.0;
        let mut unwrapped_min: f32 = 0.0;
        for group in groups {
            let mut group_min: f32 = 0.0;
            let mut group_max: f32 = 0.0;
            let mut segment: f32 = 0.0;
            let mut segment_has_glyph = false;
            let mut last_space = 0.0;
            for piece in group.pieces {
                let (min, max) = match &piece.kind {
                    PieceKind::Atomic(node) => {
                        self.content_at(*node, depth + 1);
                        let sizes = self.contribution(*node);
                        (sizes.min_content, sizes.max_content)
                    }
                    PieceKind::Tab => {
                        let style = &self.styles[piece.node];
                        let stop = text::width(" ", style.font_size, style.bold).max(0.001) * 8.0;
                        let width = stop - (line + group_max) % stop;
                        (width, width)
                    }
                    PieceKind::Text(run)
                        if group.wrap
                            && self.styles[piece.node].overflow_wrap == OverflowWrap::Anywhere =>
                    {
                        (
                            text::grapheme_advances(run)
                                .iter()
                                .map(|(_, a)| *a)
                                .fold(0.0, f32::max),
                            piece.width,
                        )
                    }
                    _ => (piece.width, piece.width),
                };
                if piece.collapsible && line + group_max == 0.0 {
                    continue;
                }
                group_max += max;
                // Anywhere takes all grapheme opportunities; normal/break-word do
                // not count emergency breaks in their min-content contribution.
                if group.wrap
                    && self.styles[piece.node].overflow_wrap == OverflowWrap::Anywhere
                    && let PieceKind::Text(run) = &piece.kind
                {
                    for (_, advance) in text::grapheme_advances(run) {
                        if segment_has_glyph {
                            group_min = group_min.max(segment);
                            segment = 0.0;
                        }
                        segment += advance;
                        segment_has_glyph = true;
                    }
                } else {
                    segment += min;
                    segment_has_glyph |=
                        matches!(piece.kind, PieceKind::Text(_) | PieceKind::Atomic(_));
                }
                last_space = if piece.collapsible { max } else { 0.0 };
            }
            line = (line + group_max).min(MAX_SIZE);
            group_min = group_min.max((segment - last_space).max(0.0));
            if group.wrap {
                minimum = minimum.max(group_min);
            } else {
                unwrapped_min = (unwrapped_min + segment).min(MAX_SIZE);
            }
            if group.forced {
                maximum = maximum.max((line - last_space).max(0.0));
                minimum = minimum.max(unwrapped_min);
                line = 0.0;
                unwrapped_min = 0.0;
            } else {
                maximum = maximum.max((line - last_space).max(0.0));
            }
        }
        IntrinsicSizes::new(minimum.max(unwrapped_min), maximum)
    }
}
