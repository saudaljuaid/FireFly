use crate::dom::{Document, NodeId, NodeKind};
use crate::style::{
    BorderStyle, BoxSizing, Color, ComputedStyle, Display, Edges, Length, TextAlign, is_visible,
};
use crate::text;
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use unicode_bidi::BidiInfo;
use unicode_segmentation::UnicodeSegmentation;

const MAX_COORD: f32 = 1_000_000.0;
const MAX_ITEMS: usize = 200_000;

#[derive(Debug, Clone, PartialEq)]
pub enum BoxKind {
    Element,
    AnonymousBlock,
    Line,
    InlineFragment,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoxGeometry {
    pub node: Option<NodeId>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub kind: BoxKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Primitive {
    Box {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        background: Option<Color>,
        border_color: Color,
        border_width: [f32; 4],
        border_style: BorderStyle,
        radius: [f32; 4],
    },
    Text {
        x: f32,
        baseline: f32,
        content: String,
        width: f32,
        size: f32,
        color: Color,
        bold: bool,
        node: NodeId,
        run: text::ResolvedRun,
    },
    Image {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        href: String,
    },
    ClipStart {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: [f32; 4],
    },
    ClipEnd,
}

impl Primitive {
    fn translate(&mut self, dx: f32, dy: f32) {
        match self {
            Self::Box { x, y, .. } | Self::Image { x, y, .. } | Self::ClipStart { x, y, .. } => {
                *x = (*x + dx).clamp(-MAX_COORD, MAX_COORD);
                *y = (*y + dy).clamp(-MAX_COORD, MAX_COORD);
            }
            Self::Text { x, baseline, .. } => {
                *x = (*x + dx).clamp(-MAX_COORD, MAX_COORD);
                *baseline = (*baseline + dy).clamp(-MAX_COORD, MAX_COORD);
            }
            Self::ClipEnd => {}
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageSource {
    pub href: String,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
pub struct Scene {
    pub width: f32,
    pub height: f32,
    pub boxes: Vec<BoxGeometry>,
    pub primitives: Vec<Primitive>,
    pub runs: Vec<TextRunGeometry>,
    pub line_boxes: Vec<LineGeometry>,
    /// A deterministic prefix was rendered after the scene/work budget ran out.
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub struct TextRunGeometry {
    pub node: NodeId,
    pub source_range: Range<usize>,
    pub layout_range: Range<usize>,
    pub text: text::ResolvedRun,
    pub x: f32,
    pub baseline: f32,
    pub line: usize,
    pub visual_order: usize,
    pub active_inline: Vec<NodeId>,
}

#[derive(Debug, Clone)]
pub struct LineGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
    pub advance: f32,
    pub runs: Range<usize>,
    pub base_direction: crate::style::Direction,
}

fn edge_values(edges: Edges, base: f32) -> [f32; 4] {
    [
        edges.top.resolve(base).unwrap_or(0.0),
        edges.right.resolve(base).unwrap_or(0.0),
        edges.bottom.resolve(base).unwrap_or(0.0),
        edges.left.resolve(base).unwrap_or(0.0),
    ]
    .map(|value| value.clamp(-MAX_COORD, MAX_COORD))
}

fn vertical_length(length: Length, containing_height: Option<f32>) -> Option<f32> {
    match length {
        Length::Px(value) => Some(value),
        Length::Percent(percent) => containing_height.map(|height| height * percent / 100.0),
        Length::Auto => None,
    }
}

fn inset(style: &ComputedStyle, base: f32) -> ([f32; 4], [f32; 4]) {
    let padding = edge_values(style.padding, base).map(|v| v.max(0.0));
    let border = if style.border_style == BorderStyle::None {
        [0.0; 4]
    } else {
        edge_values(style.border_width, base).map(|v| v.max(0.0))
    };
    (padding, border)
}

fn rounded(style: &ComputedStyle, width: f32, height: f32) -> [f32; 4] {
    let values = [
        style.border_radius.top,
        style.border_radius.right,
        style.border_radius.bottom,
        style.border_radius.left,
    ];
    values.map(|v| {
        v.resolve(width.min(height))
            .unwrap_or(0.0)
            .clamp(0.0, width.min(height) / 2.0)
    })
}

enum FlowItem {
    Start(NodeId),
    End(NodeId),
    Text(NodeId, String),
    Break(NodeId),
    Atomic(NodeId),
    Block(NodeId),
    Absolute,
}

struct Builder<'a> {
    document: &'a Document,
    styles: &'a [ComputedStyle],
    images: &'a [Option<ImageSource>],
    list_markers: &'a [Option<String>],
    boxes: Vec<BoxGeometry>,
    primitives: Vec<Primitive>,
    runs: Vec<TextRunGeometry>,
    line_boxes: Vec<LineGeometry>,
    last_flow_baseline: Option<f32>,
    lines: usize,
    truncated: bool,
    paint_groups: Vec<crate::stacking::PaintGroup>,
    inline_backgrounds: Vec<(NodeId, usize)>,
    current_group: Option<NodeId>,
    absolute_override: Option<(NodeId, crate::position::AbsoluteSize)>,
    anchors: Vec<crate::position::LayoutAnchor>,
    budget: Rc<Cell<usize>>,
    intrinsic_text_widths: Rc<RefCell<Vec<Option<f32>>>>,
}

struct Captured {
    primitives: Vec<Primitive>,
    boxes: Vec<BoxGeometry>,
    runs: Vec<TextRunGeometry>,
    lines: Vec<LineGeometry>,
    width: f32,
    height: f32,
    baseline: f32,
    paint_groups: Vec<crate::stacking::PaintGroup>,
    inline_backgrounds: Vec<(NodeId, usize)>,
    anchors: Vec<crate::position::LayoutAnchor>,
}

struct Placement {
    piece: crate::paragraph::Piece,
    atomic: Option<Captured>,
}

impl Placement {
    fn has_content(&self) -> bool {
        self.piece.width > 0.0
            || matches!(self.piece.kind, crate::paragraph::PieceKind::Text(_))
            || self
                .atomic
                .as_ref()
                .is_some_and(|atomic| atomic.height > 0.0)
    }
}

struct Line {
    x: f32,
    y: f32,
    width: f32,
    advance: f32,
    placements: Vec<Placement>,
    height_basis: Option<f32>,
    content: bool,
}

impl Line {
    fn new(x: f32, y: f32, width: f32, height_basis: Option<f32>) -> Self {
        Self {
            x,
            y,
            width,
            advance: 0.0,
            placements: Vec::new(),
            height_basis,
            content: false,
        }
    }
    fn has_content(&self) -> bool {
        self.content
    }
    fn push(&mut self, placement: Placement) {
        self.content |= placement.has_content();
        self.placements.push(placement);
    }
}

impl<'a> Builder<'a> {
    fn nested(&self) -> Builder<'a> {
        Builder {
            document: self.document,
            styles: self.styles,
            images: self.images,
            list_markers: self.list_markers,
            boxes: Vec::new(),
            primitives: Vec::new(),
            runs: Vec::new(),
            line_boxes: Vec::new(),
            last_flow_baseline: None,
            lines: 0,
            truncated: false,
            paint_groups: Vec::new(),
            inline_backgrounds: Vec::new(),
            current_group: None,
            absolute_override: None,
            anchors: Vec::new(),
            budget: self.budget.clone(),
            intrinsic_text_widths: self.intrinsic_text_widths.clone(),
        }
    }

    fn flatten(
        &mut self,
        id: NodeId,
        items: &mut Vec<FlowItem>,
        depth: usize,
        pending_ends: usize,
    ) {
        if !is_visible(self.document, self.styles, id) {
            return;
        }
        if depth > 256 || items.len() + pending_ends >= MAX_ITEMS {
            self.truncated = true;
            return;
        }
        let document = self.document;
        match &document.nodes[id].kind {
            NodeKind::Text(text) => items.push(FlowItem::Text(id, text.clone())),
            NodeKind::Element(_)
                if self.styles[id].position == crate::style::Position::Absolute =>
            {
                items.push(FlowItem::Absolute)
            }
            NodeKind::Element(element) if element.tag == "br" => items.push(FlowItem::Break(id)),
            NodeKind::Element(_) if self.styles[id].display == Display::Block => {
                items.push(FlowItem::Block(id))
            }
            NodeKind::Element(_) if self.styles[id].display == Display::InlineBlock => {
                items.push(FlowItem::Atomic(id))
            }
            NodeKind::Element(_) => {
                // Reserve this inline's end plus every already-open end so
                // item truncation leaves a balanced, bounded flow prefix.
                if items.len() + pending_ends + 2 > MAX_ITEMS {
                    self.truncated = true;
                    return;
                }
                items.push(FlowItem::Start(id));
                for &child in &document.nodes[id].children {
                    self.flatten(child, items, depth + 1, pending_ends + 1);
                }
                items.push(FlowItem::End(id));
            }
            _ => {}
        }
    }

    fn relative_inline(
        &self,
        active: &[NodeId],
        width: f32,
        height: Option<f32>,
        direction: crate::style::Direction,
    ) -> (f32, f32) {
        let mut dx = 0.0;
        let mut dy = 0.0;
        for &id in active {
            let s = &self.styles[id];
            if s.position == crate::style::Position::Relative {
                let shift = crate::position::relative_offset(s, width, height, direction);
                dx += shift.dx;
                dy += shift.dy;
            }
        }
        (
            dx.clamp(-MAX_COORD, MAX_COORD),
            dy.clamp(-MAX_COORD, MAX_COORD),
        )
    }

    fn finish_line(&mut self, line: &mut Line, parent: NodeId, forced: bool) -> f32 {
        use crate::paragraph::PieceKind;
        // Collapsible whitespace at a line edge has no advance or ink.
        let mut trailing = true;
        for placement in line.placements.iter_mut().rev() {
            if matches!(placement.piece.kind, PieceKind::Edge) {
                continue;
            }
            if trailing && placement.piece.collapsible {
                line.advance -= placement.piece.width;
                placement.piece.width = 0.0;
                placement.piece.kind = PieceKind::Edge;
            } else {
                trailing = false;
            }
        }
        line.content = line.placements.iter().any(Placement::has_content);
        if !line.has_content() && !forced {
            return line.y;
        }
        if self.lines >= MAX_ITEMS
            || self.primitives.len() >= MAX_ITEMS
            || self.runs.len() >= MAX_ITEMS
        {
            self.truncated = true;
            return line.y;
        }
        self.lines += 1;
        let parent_style = &self.styles[parent];
        let mut ascent: f32 = 0.0;
        let mut descent: f32 = 0.0;
        let only_atomic = !line.placements.is_empty()
            && line
                .placements
                .iter()
                .all(|p| matches!(p.piece.kind, PieceKind::Atomic(_) | PieceKind::Edge));
        if !only_atomic {
            let m = text::metrics(parent_style.font_size, parent_style.bold);
            let leading = (parent_style.line_height - m.ascent - m.descent) / 2.0;
            ascent = m.ascent + leading;
            descent = m.descent + leading;
        }
        for placement in &line.placements {
            match &placement.piece.kind {
                PieceKind::Text(run) => {
                    let s = &self.styles[placement.piece.node];
                    let leading = (s.line_height - run.ascent - run.descent) / 2.0;
                    ascent = ascent.max(run.ascent + leading);
                    descent = descent.max(run.descent + leading);
                }
                PieceKind::Atomic(_) => {
                    if let Some(a) = &placement.atomic {
                        ascent = ascent.max(a.baseline);
                        descent = descent.max(a.height - a.baseline);
                    }
                }
                _ => {}
            }
        }
        let height = (ascent + descent).clamp(1.0, MAX_COORD);
        let baseline = (line.y + ascent).clamp(-MAX_COORD, MAX_COORD);
        let spare = line.width - line.advance;
        let offset = match parent_style.text_align.physical(parent_style.direction) {
            TextAlign::Center => spare / 2.0,
            TextAlign::Right => spare,
            _ => 0.0,
        };
        let mut levels: Vec<_> = line.placements.iter().map(|p| p.piece.level).collect();
        // UAX #9 L1 resets trailing preserved whitespace on each wrapped line.
        let base_level = if parent_style.direction == crate::style::Direction::Rtl {
            unicode_bidi::Level::rtl()
        } else {
            unicode_bidi::Level::ltr()
        };
        for (i, placement) in line.placements.iter().enumerate().rev() {
            if matches!(placement.piece.kind, PieceKind::Edge) {
                continue;
            }
            let whitespace = match &placement.piece.kind {
                PieceKind::Text(run) => run.content.chars().all(|c| {
                    matches!(
                        unicode_bidi::bidi_class(c),
                        unicode_bidi::BidiClass::WS
                            | unicode_bidi::BidiClass::S
                            | unicode_bidi::BidiClass::B
                            | unicode_bidi::BidiClass::BN
                    )
                }),
                PieceKind::Tab => true,
                _ => false,
            };
            if !whitespace {
                break;
            }
            levels[i] = base_level;
        }
        let order = BidiInfo::reorder_visual(&levels);
        let mut positions = vec![0.0; order.len()];
        let mut cursor = line.x + offset;
        // Inline fragments are collected from visual positions, then painted before ink.
        let mut fragments: Vec<(NodeId, f32, f32, bool, bool)> = Vec::new();
        let mut last_fragment = HashMap::<NodeId, usize>::new();
        for &i in &order {
            let p = &line.placements[i].piece;
            positions[i] = cursor;
            for &id in p.active.iter() {
                let physical_left = p.node == id && p.edge_end == Some(p.level.is_rtl());
                let physical_right = p.node == id && p.edge_end == Some(!p.level.is_rtl());
                if let Some(&index) = last_fragment.get(&id)
                    && (fragments[index].2 - cursor).abs() < 0.01
                {
                    fragments[index].2 = cursor + p.width;
                    fragments[index].3 |= physical_left;
                    fragments[index].4 |= physical_right;
                } else {
                    last_fragment.insert(id, fragments.len());
                    fragments.push((id, cursor, cursor + p.width, physical_left, physical_right));
                }
            }
            cursor = (cursor + p.width).clamp(-MAX_COORD, MAX_COORD);
        }
        self.boxes.push(BoxGeometry {
            node: None,
            x: line.x,
            y: line.y,
            width: line.width,
            height,
            kind: BoxKind::Line,
        });
        for (id, start, end, physical_left, physical_right) in fragments {
            if self.boxes.len() >= MAX_ITEMS || self.primitives.len() >= MAX_ITEMS {
                self.truncated = true;
                break;
            }
            let style = &self.styles[id];
            let (padding, border) = inset(style, line.width);
            let m = text::metrics(style.font_size, style.bold);
            let leading = (style.line_height - m.ascent - m.descent) / 2.0;
            let fragment_height =
                (style.line_height + padding[0] + padding[2] + border[0] + border[2])
                    .clamp(0.0, MAX_COORD);
            let mut ancestors = Vec::new();
            let mut current = Some(id);
            while let Some(node) = current {
                if self.styles[node].display != Display::Inline {
                    break;
                }
                ancestors.push(node);
                current = self.document.nodes[node].parent;
            }
            let (dx, dy) = self.relative_inline(
                &ancestors,
                line.width,
                line.height_basis,
                parent_style.direction,
            );
            let x = (start + dx).clamp(-MAX_COORD, MAX_COORD);
            let y = (baseline - m.ascent - leading - padding[0] - border[0] + dy)
                .clamp(-MAX_COORD, MAX_COORD);
            let width = (end - start).clamp(0.0, MAX_COORD);
            self.boxes.push(BoxGeometry {
                node: Some(id),
                x,
                y,
                width,
                height: fragment_height,
                kind: BoxKind::InlineFragment,
            });
            if style.background.is_some() || style.border_style != BorderStyle::None {
                let mut fragment_border = border;
                if !physical_left {
                    fragment_border[3] = 0.0;
                }
                if !physical_right {
                    fragment_border[1] = 0.0;
                }
                let mut radius = rounded(style, width, fragment_height);
                if !physical_left {
                    radius[0] = 0.0;
                    radius[3] = 0.0;
                }
                if !physical_right {
                    radius[1] = 0.0;
                    radius[2] = 0.0;
                }
                self.inline_backgrounds.push((id, self.primitives.len()));
                self.primitives.push(Primitive::Box {
                    x,
                    y,
                    width,
                    height: fragment_height,
                    background: style.background,
                    border_color: style.border_color,
                    border_width: fragment_border,
                    border_style: style.border_style,
                    radius,
                });
            }
        }
        let run_start = self.runs.len();
        let line_index = self.line_boxes.len();
        let mut placements: Vec<_> = std::mem::take(&mut line.placements)
            .into_iter()
            .map(Some)
            .collect();
        for (visual_order, i) in order.into_iter().enumerate() {
            let placement = placements[i].take().unwrap();
            let p = placement.piece;
            let (dx, dy) = self.relative_inline(
                &p.active,
                line.width,
                line.height_basis,
                parent_style.direction,
            );
            let x = (positions[i] + dx).clamp(-MAX_COORD, MAX_COORD);
            match p.kind {
                PieceKind::Text(run) => {
                    if self.runs.len() >= MAX_ITEMS || self.primitives.len() >= MAX_ITEMS {
                        self.truncated = true;
                        break;
                    }
                    let style = &self.styles[p.node];
                    let painted_baseline = (baseline + dy).clamp(-MAX_COORD, MAX_COORD);
                    self.primitives.push(Primitive::Text {
                        x,
                        baseline: painted_baseline,
                        content: run.content.clone(),
                        width: run.advance,
                        size: run.size,
                        color: style.color,
                        bold: run.bold,
                        node: p.node,
                        run: run.clone(),
                    });
                    self.runs.push(TextRunGeometry {
                        node: p.node,
                        source_range: p.source,
                        layout_range: p.range,
                        text: run,
                        x,
                        baseline: painted_baseline,
                        line: line_index,
                        visual_order,
                        active_inline: p.active.to_vec(),
                    });
                }
                PieceKind::Atomic(_) => {
                    if let Some(mut a) = placement.atomic {
                        let ay = baseline - a.baseline + dy;
                        for primitive in &mut a.primitives {
                            primitive.translate(x, ay);
                        }
                        for geometry in &mut a.boxes {
                            geometry.x = (geometry.x + x).clamp(-MAX_COORD, MAX_COORD);
                            geometry.y = (geometry.y + ay).clamp(-MAX_COORD, MAX_COORD);
                        }
                        let base_line = self.line_boxes.len() + 1;
                        let base_run = self.runs.len();
                        for run in &mut a.runs {
                            run.x = (run.x + x).clamp(-MAX_COORD, MAX_COORD);
                            run.baseline = (run.baseline + ay).clamp(-MAX_COORD, MAX_COORD);
                            run.line += base_line;
                        }
                        for l in &mut a.lines {
                            l.x = (l.x + x).clamp(-MAX_COORD, MAX_COORD);
                            l.y = (l.y + ay).clamp(-MAX_COORD, MAX_COORD);
                            l.baseline = (l.baseline + ay).clamp(-MAX_COORD, MAX_COORD);
                            l.runs = l.runs.start + base_run..l.runs.end + base_run;
                        }
                        let paint_offset = self.primitives.len();
                        for group in &mut a.paint_groups {
                            group.start += paint_offset;
                            group.content_start += paint_offset;
                            group.content_end += paint_offset;
                            group.end += paint_offset;
                            if group.parent.is_none() {
                                group.parent = self.current_group;
                            }
                        }
                        for anchor in &mut a.anchors {
                            anchor.translate(x, ay);
                        }
                        for (_, index) in &mut a.inline_backgrounds {
                            *index += paint_offset;
                        }
                        self.inline_backgrounds.extend(a.inline_backgrounds);
                        self.paint_groups.extend(a.paint_groups);
                        self.anchors.extend(a.anchors);
                        self.primitives.extend(a.primitives);
                        self.boxes.extend(a.boxes);
                        self.runs.extend(a.runs);
                        // Keep this line first so nested line indices remain stable.
                        // Nested lines are appended after the parent below.
                        for l in a.lines {
                            self.line_boxes.push(l);
                        }
                    }
                }
                _ => {}
            }
        }
        self.line_boxes.insert(
            line_index,
            LineGeometry {
                x: line.x,
                y: line.y,
                width: line.width,
                height,
                baseline,
                advance: line.advance.max(0.0),
                runs: run_start..self.runs.len(),
                base_direction: parent_style.direction,
            },
        );
        self.last_flow_baseline = Some(baseline);
        (line.y + height).min(MAX_COORD)
    }

    fn capture_atomic(
        &mut self,
        id: NodeId,
        width: f32,
        containing_height: Option<f32>,
        depth: usize,
    ) -> Captured {
        let mut nested = self.nested();
        nested.current_group = self.current_group;
        let (w, h) = nested.block(id, (0.0, 0.0), width, containing_height, depth + 1, true);
        let baseline = if self.styles[id].overflow_hidden
            || self
                .document
                .element(id)
                .is_some_and(|element| element.tag == "img")
        {
            h
        } else {
            nested.last_flow_baseline.unwrap_or(h)
        };
        self.truncated |= nested.truncated;
        Captured {
            primitives: nested.primitives,
            boxes: nested.boxes,
            runs: nested.runs,
            lines: nested.line_boxes,
            width: w,
            height: h,
            baseline,
            paint_groups: nested.paint_groups,
            inline_backgrounds: nested.inline_backgrounds,
            anchors: nested.anchors,
        }
    }

    fn layout_paragraph(
        &mut self,
        paragraph: crate::paragraph::Paragraph,
        parent: NodeId,
        origin: (f32, f32),
        width: f32,
        containing_height: Option<f32>,
        depth: usize,
    ) -> f32 {
        use crate::paragraph::{Piece, PieceKind};
        if paragraph.is_empty() {
            return origin.1;
        }
        self.truncated |= paragraph.truncated;
        let groups = paragraph.prepare(self.styles, self.styles[parent].direction);
        let mut line = Line::new(origin.0, origin.1, width, containing_height);
        'groups: for mut group in groups {
            if self.lines >= MAX_ITEMS || self.primitives.len() >= MAX_ITEMS || line.y >= MAX_COORD
            {
                self.truncated = true;
                break;
            }
            let mut prepared = Vec::new();
            let mut anticipated = line.advance;
            for mut piece in group.pieces.drain(..) {
                let atomic = if let PieceKind::Atomic(id) = piece.kind {
                    let a = self.capture_atomic(id, width, containing_height, depth);
                    piece.width = a.width;
                    Some(a)
                } else {
                    None
                };
                if matches!(piece.kind, PieceKind::Tab) {
                    let style = &self.styles[piece.node];
                    let space = text::width(" ", style.font_size, style.bold).max(0.001);
                    let stop = space * 8.0;
                    piece.width = stop - (anticipated % stop);
                }
                anticipated += piece.width;
                prepared.push(Placement { piece, atomic });
            }
            let mut group_width: f32 = prepared.iter().map(|p| p.piece.width).sum();
            for placement in prepared.iter().rev() {
                if matches!(placement.piece.kind, PieceKind::Edge) {
                    continue;
                }
                if placement.piece.collapsible {
                    group_width -= placement.piece.width;
                } else {
                    break;
                }
            }
            if group.wrap && line.has_content() && line.advance + group_width > width {
                let y = self.finish_line(&mut line, parent, false);
                line = Line::new(origin.0, y, width, containing_height);
            }
            for mut placement in prepared {
                if matches!(placement.piece.kind, PieceKind::Tab) {
                    let style = &self.styles[placement.piece.node];
                    let mut resolved = text::resolve(
                        " ",
                        style.font_size,
                        style.bold,
                        placement.piece.level.is_rtl(),
                    );
                    if let Some(mut gap) = resolved.pop() {
                        let stop = gap.advance.max(0.001) * 8.0;
                        gap.advance = stop - (line.advance % stop);
                        if group.wrap && line.has_content() && line.advance + gap.advance > width {
                            let y = self.finish_line(&mut line, parent, false);
                            line = Line::new(origin.0, y, width, containing_height);
                            gap.advance = stop;
                        }
                        gap.content = "\t".into();
                        for glyph in &mut gap.glyphs {
                            glyph.advance = gap.advance;
                        }
                        placement.piece.width = gap.advance;
                        placement.piece.kind = PieceKind::Text(gap);
                    }
                }
                let p = &placement.piece;
                if p.collapsible && !line.has_content() {
                    continue;
                }
                let emergency = group.wrap
                    && (!group.glue
                        || self.styles[p.node].overflow_wrap
                            == crate::style::OverflowWrap::Anywhere)
                    && self.styles[p.node].overflow_wrap != crate::style::OverflowWrap::Normal
                    && !matches!(&p.kind, PieceKind::Text(run) if run.content == "\t");
                if emergency
                    && p.width > width
                    && let PieceKind::Text(run) = &p.kind
                {
                    // Emergency fragments are reshaped before placement; a cluster is never split.
                    let mut chunk = String::new();
                    let mut chunk_width = 0.0;
                    let mut offset = 0;
                    let mut chunks = Vec::new();
                    let graphemes: Vec<_> = run.content.grapheme_indices(true).collect();
                    let mut advances = vec![0.0; graphemes.len()];
                    let mut clusters: Vec<_> = run
                        .glyphs
                        .iter()
                        .map(|glyph| (glyph.cluster, glyph.advance))
                        .collect();
                    // Stable sorting preserves the shaped glyph addition order within
                    // each cluster without allocating a tree node for every glyph.
                    clusters.sort_by_key(|&(cluster, _)| cluster);
                    let mut distinct = 0;
                    for read in 0..clusters.len() {
                        let (cluster, advance) = clusters[read];
                        if distinct == 0 || clusters[distinct - 1].0 != cluster {
                            clusters[distinct] = (cluster, 0.0);
                            distinct += 1;
                        }
                        clusters[distinct - 1].1 += advance;
                    }
                    clusters.truncate(distinct);
                    for (i, &(cluster, advance)) in clusters.iter().enumerate() {
                        let start = graphemes
                            .partition_point(|(offset, _)| *offset <= cluster)
                            .saturating_sub(1);
                        let next = clusters
                            .get(i + 1)
                            .map_or(run.content.len(), |&(offset, _)| offset);
                        let end = graphemes
                            .partition_point(|(offset, _)| *offset < next)
                            .max(start + 1)
                            .min(graphemes.len());
                        let share = advance / (end - start) as f32;
                        for value in &mut advances[start..end] {
                            *value += share;
                        }
                    }
                    for ((_, g), advance) in graphemes.into_iter().zip(advances) {
                        if !chunk.is_empty() && chunk_width + advance > width.max(0.0) {
                            chunks.push((std::mem::take(&mut chunk), offset));
                            offset += chunks.last().unwrap().0.len();
                            chunk_width = 0.0;
                        }
                        chunk.push_str(g);
                        chunk_width += advance;
                        if chunks.len() >= MAX_ITEMS {
                            self.truncated = true;
                            break;
                        }
                    }
                    if !chunk.is_empty() {
                        chunks.push((chunk, offset));
                    }
                    for (chunk, offset) in chunks {
                        if line.y >= MAX_COORD {
                            self.truncated = true;
                            break;
                        }
                        for resolved in text::resolve(
                            &chunk,
                            run.size,
                            run.bold,
                            run.direction == text::Direction::Rtl,
                        ) {
                            if line.has_content() && line.advance + resolved.advance > width {
                                let y = self.finish_line(&mut line, parent, false);
                                line = Line::new(origin.0, y, width, containing_height);
                            }
                            if line.advance + resolved.advance
                                > (MAX_COORD - line.x.max(0.0)).max(0.0)
                            {
                                self.truncated = true;
                                break 'groups;
                            }
                            let length = resolved.content.len();
                            let piece = Piece {
                                kind: PieceKind::Text(resolved.clone()),
                                width: resolved.advance,
                                range: p.range.start + offset..p.range.start + offset + length,
                                source: if p.source.len() == run.content.len() {
                                    p.source.start + offset..p.source.start + offset + length
                                } else {
                                    p.source.clone()
                                },
                                node: p.node,
                                active: p.active.clone(),
                                level: p.level,
                                collapsible: false,
                                edge_end: None,
                            };
                            line.advance += piece.width;
                            line.push(Placement {
                                piece,
                                atomic: None,
                            });
                        }
                    }
                } else {
                    if emergency && line.has_content() && line.advance + p.width > width {
                        let y = self.finish_line(&mut line, parent, false);
                        line = Line::new(origin.0, y, width, containing_height);
                    }
                    if p.collapsible && !line.has_content() {
                        continue;
                    }
                    if line.advance + p.width > (MAX_COORD - line.x.max(0.0)).max(0.0) {
                        self.truncated = true;
                        break 'groups;
                    }
                    line.advance += p.width;
                    line.push(placement);
                }
            }
            if group.forced {
                let y = self.finish_line(&mut line, parent, true);
                line = Line::new(origin.0, y, width, containing_height);
            }
        }
        self.finish_line(&mut line, parent, false)
    }

    fn children(
        &mut self,
        id: NodeId,
        x: f32,
        y: f32,
        width: f32,
        containing_height: Option<f32>,
        depth: usize,
    ) -> f32 {
        use crate::style::UnicodeBidi;
        if depth > 256 || self.primitives.len() >= MAX_ITEMS {
            self.truncated = true;
            return 0.0;
        }
        let mut items = Vec::new();
        for &child in &self.document.nodes[id].children {
            self.flatten(child, &mut items, depth + 1, 0);
        }
        let mut paragraph = crate::paragraph::Paragraph::new(self.budget.clone());
        let mut active = Vec::new();
        let mut bottom = y;
        let mut run_start = y;
        for item in items {
            if self.lines >= MAX_ITEMS || bottom >= MAX_COORD {
                self.truncated = true;
                break;
            }
            match item {
                FlowItem::Start(child) => {
                    let style = &self.styles[child];
                    active.push(child);
                    match style.unicode_bidi {
                        UnicodeBidi::Embed => paragraph.special(
                            if style.direction == crate::style::Direction::Rtl {
                                '\u{202b}'
                            } else {
                                '\u{202a}'
                            },
                            child,
                            &active,
                            false,
                            true,
                        ),
                        UnicodeBidi::Isolate => paragraph.special(
                            if style.direction == crate::style::Direction::Rtl {
                                '\u{2067}'
                            } else {
                                '\u{2066}'
                            },
                            child,
                            &active,
                            false,
                            true,
                        ),
                        _ => {}
                    }
                    let (padding, border) = inset(style, width);
                    let margin = edge_values(style.margin, width);
                    paragraph.edge(
                        child,
                        &active,
                        (
                            padding[3] + border[3] + margin[3],
                            padding[1] + border[1] + margin[1],
                        ),
                        false,
                    );
                }
                FlowItem::End(child) => {
                    let style = &self.styles[child];
                    let (padding, border) = inset(style, width);
                    let margin = edge_values(style.margin, width);
                    paragraph.edge(
                        child,
                        &active,
                        (
                            padding[3] + border[3] + margin[3],
                            padding[1] + border[1] + margin[1],
                        ),
                        true,
                    );
                    match style.unicode_bidi {
                        UnicodeBidi::Embed => {
                            paragraph.special('\u{202c}', child, &active, false, true)
                        }
                        UnicodeBidi::Isolate => {
                            paragraph.special('\u{2069}', child, &active, false, true)
                        }
                        _ => {}
                    }
                    if active.last() == Some(&child) {
                        active.pop();
                    }
                }
                FlowItem::Text(child, source) => {
                    paragraph.text(&source, child, &active, &self.styles[child])
                }
                FlowItem::Break(child) => paragraph.special('\n', child, &active, false, false),
                FlowItem::Atomic(child) => {
                    paragraph.special('\u{fffc}', child, &active, true, false)
                }
                FlowItem::Absolute => {} // Positioned descendants are resolved in the second layout pass.
                FlowItem::Block(child) => {
                    let next_paragraph = crate::paragraph::Paragraph::new(self.budget.clone());
                    bottom = self.layout_paragraph(
                        std::mem::replace(&mut paragraph, next_paragraph),
                        id,
                        (x, bottom),
                        width,
                        containing_height,
                        depth,
                    );
                    if bottom > run_start {
                        self.boxes.push(BoxGeometry {
                            node: None,
                            x,
                            y: run_start,
                            width,
                            height: bottom - run_start,
                            kind: BoxKind::AnonymousBlock,
                        });
                    }
                    let (_, height) = self.block(
                        child,
                        (x, bottom),
                        width,
                        containing_height,
                        depth + 1,
                        false,
                    );
                    bottom = (bottom + height).min(MAX_COORD);
                    run_start = bottom;
                }
            }
        }
        bottom = self.layout_paragraph(paragraph, id, (x, bottom), width, containing_height, depth);
        if bottom > run_start {
            self.boxes.push(BoxGeometry {
                node: None,
                x,
                y: run_start,
                width,
                height: bottom - run_start,
                kind: BoxKind::AnonymousBlock,
            });
        }
        (bottom - y).max(0.0)
    }

    fn block(
        &mut self,
        id: NodeId,
        origin: (f32, f32),
        available: f32,
        containing_height: Option<f32>,
        depth: usize,
        atomic: bool,
    ) -> (f32, f32) {
        let (x, y) = origin;
        if depth > 256 || self.primitives.len() >= MAX_ITEMS {
            self.truncated = true;
            return (0.0, 0.0);
        }
        let remaining = self.budget.get();
        if remaining == 0 {
            self.truncated = true;
            return (0.0, 0.0);
        }
        self.budget.set(remaining - 1);
        let box_start = self.boxes.len();
        let run_start = self.runs.len();
        let line_start = self.line_boxes.len();
        let anchor_start = self.anchors.len();
        let group_start = self.paint_groups.len();
        let inline_background_start = self.inline_backgrounds.len();
        let parent_group = self.current_group;
        self.current_group = Some(id);
        let style = &self.styles[id];
        let absolute_size = self
            .absolute_override
            .filter(|(node, _)| *node == id)
            .map(|(_, size)| size);
        let available = available.clamp(0.0, MAX_COORD);
        let (padding, border) = inset(style, available);
        let horizontal_inset = padding[1] + padding[3] + border[1] + border[3];
        let vertical_inset = padding[0] + padding[2] + border[0] + border[2];
        let margins = edge_values(style.margin, available);
        let image = self.images.get(id).and_then(|image| image.as_ref());
        let is_image = self
            .document
            .element(id)
            .is_some_and(|element| element.tag == "img");
        let html_dimension = |name: &str| {
            self.document
                .element(id)
                .and_then(|element| element.attribute(name))
                .and_then(|value| value.trim().parse::<f32>().ok())
                .filter(|value| value.is_finite() && (0.0..=16_384.0).contains(value))
        };
        let html_width = if is_image {
            html_dimension("width")
        } else {
            None
        };
        let html_height = if is_image {
            html_dimension("height")
        } else {
            None
        };
        let css_width = absolute_size
            .and_then(|size| size.content_width)
            .or_else(|| {
                style
                    .width
                    .and_then(|value| value.resolve(available))
                    .map(|specified| {
                        if style.box_sizing == BoxSizing::BorderBox {
                            specified - horizontal_inset
                        } else {
                            specified
                        }
                    })
            });
        let css_height = absolute_size
            .and_then(|size| size.content_height)
            .or_else(|| {
                style
                    .height
                    .and_then(|value| vertical_length(value, containing_height))
                    .map(|specified| {
                        if style.box_sizing == BoxSizing::BorderBox {
                            specified - vertical_inset
                        } else {
                            specified
                        }
                    })
            });
        let mut content_width = css_width.or(html_width).or_else(|| {
            image
                .zip(css_height.or(html_height))
                .map(|(image, height)| image.width * height / image.height.max(1.0))
        });
        if content_width.is_none() && atomic {
            content_width = Some(
                self.intrinsic_width(id)
                    .min((available - horizontal_inset).max(0.0)),
            );
        }
        let auto_width = content_width.is_none();
        let left = if style.margin.left.resolve(available).is_none() {
            0.0
        } else {
            margins[3]
        };
        let right = if style.margin.right.resolve(available).is_none() {
            0.0
        } else {
            margins[1]
        };
        let mut content_width =
            content_width.unwrap_or((available - left - right - horizontal_inset).max(0.0));
        let constraint = |value: Option<crate::style::Length>| {
            value.and_then(|v| v.resolve(available)).map(|v| {
                if style.box_sizing == BoxSizing::BorderBox {
                    v - horizontal_inset
                } else {
                    v
                }
            })
        };
        if let Some(max) = constraint(style.max_width) {
            content_width = content_width.min(max);
        }
        if let Some(min) = constraint(style.min_width) {
            content_width = content_width.max(min);
        }
        content_width = content_width.clamp(0.0, MAX_COORD);
        let remaining = (available - content_width - horizontal_inset - left - right).max(0.0);
        let (margin_left, margin_right) = if auto_width {
            (left, right)
        } else {
            match (
                style.margin.left.resolve(available).is_none(),
                style.margin.right.resolve(available).is_none(),
            ) {
                (true, true) => (remaining / 2.0, remaining / 2.0),
                (true, false) => (remaining, right),
                (false, true) => (left, remaining),
                _ => (left, right),
            }
        };
        let outer_x = (x + margin_left).clamp(-MAX_COORD, MAX_COORD);
        let outer_y = (y + margins[0]).clamp(-MAX_COORD, MAX_COORD);
        let border_width = (content_width + horizontal_inset).clamp(0.0, MAX_COORD);
        let content_x = (outer_x + border[3] + padding[3]).clamp(-MAX_COORD, MAX_COORD);
        let content_y = (outer_y + border[0] + padding[0]).clamp(-MAX_COORD, MAX_COORD);
        let paint_index = self.primitives.len();
        let child_height = if is_image {
            0.0
        } else {
            self.children(
                id,
                content_x,
                content_y,
                content_width,
                css_height.or(html_height),
                depth + 1,
            )
        };
        let child_height = if self.list_markers[id].is_some() {
            child_height.max(style.line_height)
        } else {
            child_height
        };
        let mut content_height = css_height.or(html_height).unwrap_or_else(|| {
            if let Some(image) = image {
                image.height * content_width / image.width.max(1.0)
            } else if is_image {
                style.line_height
            } else {
                child_height
            }
        });
        let height_constraint = |value: Option<Length>| {
            value
                .and_then(|length| vertical_length(length, containing_height))
                .map(|value| {
                    if style.box_sizing == BoxSizing::BorderBox {
                        (value - vertical_inset).max(0.0)
                    } else {
                        value
                    }
                })
        };
        if let Some(max) = height_constraint(style.max_height) {
            content_height = content_height.min(max);
        }
        if let Some(min) = height_constraint(style.min_height) {
            content_height = content_height.max(min);
        }
        content_height = content_height.clamp(0.0, MAX_COORD);
        let border_height = (content_height + vertical_inset).clamp(0.0, MAX_COORD);
        self.boxes.push(BoxGeometry {
            node: Some(id),
            x: outer_x,
            y: outer_y,
            width: border_width,
            height: border_height,
            kind: BoxKind::Element,
        });
        let has_box_paint = style.background.is_some() || style.border_style != BorderStyle::None;
        if has_box_paint {
            self.primitives.insert(
                paint_index,
                Primitive::Box {
                    x: outer_x,
                    y: outer_y,
                    width: border_width,
                    height: border_height,
                    background: style.background,
                    border_color: style.border_color,
                    border_width: border,
                    border_style: style.border_style,
                    radius: rounded(style, border_width, border_height),
                },
            );
            for group in &mut self.paint_groups[group_start..] {
                group.offset(1);
            }
            for (_, index) in &mut self.inline_backgrounds[inline_background_start..] {
                *index += 1;
            }
        }
        if let Some(image) = image {
            self.primitives.push(Primitive::Image {
                x: content_x,
                y: content_y,
                width: content_width,
                height: content_height,
                href: image.href.clone(),
            });
        } else if is_image
            && let Some(alt) = self
                .document
                .element(id)
                .and_then(|element| element.attribute("alt"))
            && !alt.is_empty()
        {
            let mut x = content_x;
            let mut remaining = self.budget.get();
            let mut alt_end = 0;
            for grapheme in alt.graphemes(true) {
                let count = grapheme.chars().count();
                if count > remaining {
                    break;
                }
                remaining -= count;
                alt_end += grapheme.len();
            }
            self.budget.set(remaining);
            self.truncated |= alt_end < alt.len();
            let resolved = text::resolve(
                &alt[..alt_end],
                style.font_size,
                style.bold,
                style.direction == crate::style::Direction::Rtl,
            );
            let ascent = resolved.iter().map(|run| run.ascent).fold(0.0, f32::max);
            let descent = resolved.iter().map(|run| run.descent).fold(0.0, f32::max);
            let leading = (style.line_height - ascent - descent).max(0.0) / 2.0;
            let baseline = (content_y + ascent + leading).clamp(-MAX_COORD, MAX_COORD);
            let line_index = self.line_boxes.len();
            let alt_start = self.runs.len();
            let mut source_offset = 0;
            for run in resolved {
                if self.lines >= MAX_ITEMS
                    || self.line_boxes.len() >= MAX_ITEMS
                    || self.primitives.len() >= MAX_ITEMS
                    || self.runs.len() >= MAX_ITEMS
                {
                    self.truncated = true;
                    break;
                }
                if x + run.advance > MAX_COORD {
                    self.truncated = true;
                    break;
                }
                let source_end = source_offset + run.content.len();
                self.primitives.push(Primitive::Text {
                    x,
                    baseline,
                    content: run.content.clone(),
                    width: run.advance,
                    size: run.size,
                    color: style.color,
                    bold: run.bold,
                    node: id,
                    run: run.clone(),
                });
                self.runs.push(TextRunGeometry {
                    node: id,
                    source_range: source_offset..source_end,
                    layout_range: source_offset..source_end,
                    text: run.clone(),
                    x,
                    baseline,
                    line: line_index,
                    visual_order: self.runs.len() - alt_start,
                    active_inline: Vec::new(),
                });
                source_offset = source_end;
                x = (x + run.advance).min(MAX_COORD);
            }
            if self.runs.len() > alt_start {
                self.lines += 1;
                let height = (ascent + descent + leading * 2.0).clamp(0.0, MAX_COORD);
                self.line_boxes.push(LineGeometry {
                    x: content_x,
                    y: content_y,
                    width: content_width,
                    height,
                    baseline,
                    advance: (x - content_x).clamp(0.0, MAX_COORD),
                    runs: alt_start..self.runs.len(),
                    base_direction: style.direction,
                });
                self.boxes.push(BoxGeometry {
                    node: None,
                    x: content_x,
                    y: content_y,
                    width: content_width,
                    height,
                    kind: BoxKind::Line,
                });
            }
        }
        self.paint_list_marker(id, (content_x, content_y), content_width, line_start);
        if style.overflow_hidden {
            let outer_radius = rounded(style, border_width, border_height);
            let clip_radius = [
                (outer_radius[0] - border[0].max(border[3])).max(0.0),
                (outer_radius[1] - border[0].max(border[1])).max(0.0),
                (outer_radius[2] - border[2].max(border[1])).max(0.0),
                (outer_radius[3] - border[2].max(border[3])).max(0.0),
            ];
            self.primitives.insert(
                paint_index + usize::from(has_box_paint),
                Primitive::ClipStart {
                    x: (outer_x + border[3]).clamp(-MAX_COORD, MAX_COORD),
                    y: (outer_y + border[0]).clamp(-MAX_COORD, MAX_COORD),
                    width: (border_width - border[3] - border[1]).max(0.0),
                    height: (border_height - border[0] - border[2]).max(0.0),
                    radius: clip_radius,
                },
            );
            self.primitives.push(Primitive::ClipEnd);
            for group in &mut self.paint_groups[group_start..] {
                group.offset(1);
            }
            for (_, index) in &mut self.inline_backgrounds[inline_background_start..] {
                *index += 1;
            }
        }
        self.anchors.push(crate::position::LayoutAnchor {
            node: id,
            padding: crate::position::ContainingBlock {
                x: (outer_x + border[3]).clamp(-MAX_COORD, MAX_COORD),
                y: (outer_y + border[0]).clamp(-MAX_COORD, MAX_COORD),
                width: (border_width - border[1] - border[3]).max(0.0),
                height: Some((border_height - border[0] - border[2]).max(0.0)),
            },
            content_origin: (content_x, content_y),
            content_width,
        });
        self.paint_groups.push(crate::stacking::PaintGroup {
            node: id,
            parent: parent_group,
            start: paint_index,
            content_start: paint_index
                + usize::from(has_box_paint)
                + usize::from(style.overflow_hidden),
            content_end: self.primitives.len() - usize::from(style.overflow_hidden),
            end: self.primitives.len(),
        });
        if style.position == crate::style::Position::Relative {
            let direction =
                parent_group.map_or(style.direction, |parent| self.styles[parent].direction);
            let offset =
                crate::position::relative_offset(style, available, containing_height, direction);
            self.translate_subtree(
                (paint_index, box_start, run_start, line_start, anchor_start),
                offset.dx,
                offset.dy,
            );
        }
        self.current_group = parent_group;
        let outer_height = (margins[0] + border_height + margins[2]).clamp(0.0, MAX_COORD);
        (
            (border_width + margin_left + margin_right).clamp(0.0, MAX_COORD),
            outer_height,
        )
    }

    fn paint_list_marker(&mut self, id: NodeId, origin: (f32, f32), width: f32, first_line: usize) {
        let Some(marker) = &self.list_markers[id] else {
            return;
        };
        let style = &self.styles[id];
        let mut resolved = text::resolve(marker, style.font_size, style.bold, false);
        let Some(run) = resolved.pop() else {
            return;
        };
        let count = marker.chars().count();
        if count > self.budget.get()
            || self.runs.len() >= MAX_ITEMS
            || self.primitives.len() >= MAX_ITEMS
        {
            self.truncated = true;
            return;
        }
        self.budget.set(self.budget.get() - count);
        let (line, baseline) = if let Some(line) = self.line_boxes.get(first_line) {
            (first_line, line.baseline)
        } else {
            let baseline =
                (origin.1 + run.ascent + (style.line_height - run.ascent - run.descent) / 2.0)
                    .clamp(-MAX_COORD, MAX_COORD);
            let line = self.line_boxes.len();
            self.line_boxes.push(LineGeometry {
                x: origin.0,
                y: origin.1,
                width,
                height: style.line_height,
                baseline,
                advance: 0.0,
                runs: self.runs.len()..self.runs.len() + 1,
                base_direction: style.direction,
            });
            self.last_flow_baseline = Some(baseline);
            (line, baseline)
        };
        let x = (origin.0 - run.advance - style.font_size * 0.4).clamp(-MAX_COORD, MAX_COORD);
        self.primitives.push(Primitive::Text {
            x,
            baseline,
            content: run.content.clone(),
            width: run.advance,
            size: run.size,
            color: style.color,
            bold: run.bold,
            node: id,
            run: run.clone(),
        });
        self.runs.push(TextRunGeometry {
            node: id,
            source_range: 0..0,
            layout_range: 0..0,
            text: run,
            x,
            baseline,
            line,
            visual_order: 0,
            active_inline: Vec::new(),
        });
    }

    fn translate_subtree(&mut self, starts: (usize, usize, usize, usize, usize), dx: f32, dy: f32) {
        for primitive in &mut self.primitives[starts.0..] {
            primitive.translate(dx, dy);
        }
        for geometry in &mut self.boxes[starts.1..] {
            geometry.x = (geometry.x + dx).clamp(-MAX_COORD, MAX_COORD);
            geometry.y = (geometry.y + dy).clamp(-MAX_COORD, MAX_COORD);
        }
        for run in &mut self.runs[starts.2..] {
            run.x = (run.x + dx).clamp(-MAX_COORD, MAX_COORD);
            run.baseline = (run.baseline + dy).clamp(-MAX_COORD, MAX_COORD);
        }
        for line in &mut self.line_boxes[starts.3..] {
            line.x = (line.x + dx).clamp(-MAX_COORD, MAX_COORD);
            line.y = (line.y + dy).clamp(-MAX_COORD, MAX_COORD);
            line.baseline = (line.baseline + dy).clamp(-MAX_COORD, MAX_COORD);
        }
        for anchor in &mut self.anchors[starts.4..] {
            anchor.translate(dx, dy);
        }
    }

    fn positioned(&mut self, viewport_width: f32) {
        use crate::position::{ContainingBlock, absolute_origin, absolute_size};
        use crate::style::{Direction, Position};
        use std::collections::{HashMap, HashSet};
        // Visible DOM preorder places absolute ancestors before their absolute
        // descendants. Template/display:none ancestors stop this walk entirely.
        let mut pending = vec![0];
        let mut absolute = Vec::new();
        while let Some(node) = pending.pop() {
            if !is_visible(self.document, self.styles, node) {
                continue;
            }
            if self.styles[node].position == Position::Absolute {
                absolute.push(node);
                if absolute.len() >= MAX_ITEMS {
                    self.truncated = true;
                    break;
                }
            }
            pending.extend(self.document.nodes[node].children.iter().rev().copied());
        }
        let mut anchors: HashMap<_, _> = self
            .anchors
            .iter()
            .enumerate()
            .map(|(index, anchor)| (anchor.node, index))
            .collect();
        let mut groups: HashSet<_> = self.paint_groups.iter().map(|group| group.node).collect();
        let initial_direction = self
            .document
            .nodes
            .iter()
            .enumerate()
            .find_map(|(id, node)| {
                matches!(&node.kind, NodeKind::Element(element) if element.tag == "html")
                    .then_some(self.styles[id].direction)
            })
            .unwrap_or(Direction::Ltr);
        for id in absolute {
            if self.budget.get() == 0
                || self.primitives.len() >= MAX_ITEMS
                || self.boxes.len() >= MAX_ITEMS
            {
                self.truncated = true;
                break;
            }
            let mut containing = ContainingBlock {
                x: 0.0,
                y: 0.0,
                width: viewport_width,
                height: None,
            };
            let mut direction = initial_direction;
            let mut static_origin = (0.0, 0.0);
            let mut parent_group = None;
            let mut containing_found = false;
            let mut static_found = false;
            let mut ancestor = self.document.nodes[id].parent;
            while let Some(node) = ancestor {
                if parent_group.is_none() && groups.contains(&node) {
                    parent_group = Some(node);
                }
                if let Some(&index) = anchors.get(&node) {
                    let anchor = self.anchors[index];
                    if !static_found {
                        static_origin = anchor.content_origin;
                        static_found = true;
                    }
                    if !containing_found && self.styles[node].position != Position::Static {
                        containing = anchor.padding;
                        direction = self.styles[node].direction;
                        containing_found = true;
                    }
                }
                if containing_found && static_found && parent_group.is_some() {
                    break;
                }
                ancestor = self.document.nodes[node].parent;
            }
            let intrinsic = self.intrinsic_width(id);
            let style = &self.styles[id];
            let image = self.images.get(id).and_then(|image| image.as_ref());
            // HTML image dimensions remain explicit content dimensions. Model
            // them as equivalent CSS sizes only for this resolver, so an auto
            // absolute width does not shrink an explicit HTML width or block
            // the existing one-dimension intrinsic-ratio calculation.
            let mut replaced_style = None;
            if image.is_some()
                && self
                    .document
                    .element(id)
                    .is_some_and(|element| element.tag == "img")
            {
                let html_dimension = |name: &str| {
                    self.document
                        .element(id)
                        .and_then(|element| element.attribute(name))
                        .and_then(|value| value.trim().parse::<f32>().ok())
                        .filter(|value| value.is_finite() && (0.0..=16_384.0).contains(value))
                };
                let (padding, border) = inset(style, containing.width);
                let mut resolved = style.clone();
                if style
                    .width
                    .and_then(|length| length.resolve(containing.width))
                    .is_none()
                    && let Some(width) = html_dimension("width")
                {
                    let inset = if style.box_sizing == BoxSizing::BorderBox {
                        padding[1] + padding[3] + border[1] + border[3]
                    } else {
                        0.0
                    };
                    resolved.width = Some(Length::Px((width + inset).clamp(0.0, MAX_COORD)));
                }
                if style
                    .height
                    .and_then(|length| vertical_length(length, containing.height))
                    .is_none()
                    && let Some(height) = html_dimension("height")
                {
                    let inset = if style.box_sizing == BoxSizing::BorderBox {
                        padding[0] + padding[2] + border[0] + border[2]
                    } else {
                        0.0
                    };
                    resolved.height = Some(Length::Px((height + inset).clamp(0.0, MAX_COORD)));
                }
                replaced_style = Some(resolved);
            }
            let sizing_style = replaced_style.as_ref().unwrap_or(style);
            let mut size = absolute_size(sizing_style, containing, intrinsic);
            if let Some(image) = image
                && sizing_style
                    .width
                    .and_then(|length| length.resolve(containing.width))
                    .is_none()
                && let Some(height) = size.content_height
            {
                let preferred_width =
                    (image.width * height / image.height.max(1.0)).clamp(0.0, MAX_COORD);
                size = absolute_size(sizing_style, containing, preferred_width);
            }
            let mut nested = self.nested();
            nested.absolute_override = Some((id, size));
            nested.current_group = parent_group;
            nested.block(
                id,
                (0.0, 0.0),
                containing.width,
                containing.height,
                0,
                false,
            );
            let Some(geometry) = nested
                .boxes
                .iter()
                .find(|geometry| geometry.node == Some(id) && geometry.kind == BoxKind::Element)
            else {
                self.truncated = true;
                break;
            };
            let target = absolute_origin(
                &self.styles[id],
                containing,
                static_origin,
                (geometry.width, geometry.height),
                direction,
            )
            .border_origin();
            let dx = target.0 - geometry.x;
            let dy = target.1 - geometry.y;
            nested.translate_subtree((0, 0, 0, 0, 0), dx, dy);
            let Builder {
                mut paint_groups,
                mut inline_backgrounds,
                mut runs,
                mut line_boxes,
                anchors: nested_anchors,
                primitives,
                boxes,
                truncated,
                ..
            } = nested;
            let primitive_base = self.primitives.len();
            let run_base = self.runs.len();
            let line_base = self.line_boxes.len();
            for group in &mut paint_groups {
                group.offset(primitive_base);
            }
            for (_, index) in &mut inline_backgrounds {
                *index += primitive_base;
            }
            for run in &mut runs {
                run.line += line_base;
            }
            for line in &mut line_boxes {
                line.runs = line.runs.start + run_base..line.runs.end + run_base;
            }
            self.truncated |= truncated;
            for anchor in nested_anchors {
                anchors.insert(anchor.node, self.anchors.len());
                self.anchors.push(anchor);
            }
            groups.extend(paint_groups.iter().map(|group| group.node));
            self.primitives.extend(primitives);
            self.boxes.extend(boxes);
            self.runs.extend(runs);
            self.line_boxes.extend(line_boxes);
            self.paint_groups.extend(paint_groups);
            self.inline_backgrounds.extend(inline_backgrounds);
        }
    }

    fn intrinsic_width(&self, id: NodeId) -> f32 {
        if let Some(width) = self
            .document
            .element(id)
            .filter(|element| element.tag == "img")
            .and_then(|element| element.attribute("width"))
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| value.is_finite() && (0.0..=16_384.0).contains(value))
        {
            return width;
        }
        if let Some(image) = self.images.get(id).and_then(|image| image.as_ref()) {
            return image.width;
        }
        if let Some(alt) = self
            .document
            .element(id)
            .filter(|element| element.tag == "img")
            .and_then(|element| element.attribute("alt"))
        {
            return text::width(alt, self.styles[id].font_size, self.styles[id].bold);
        }
        let mut width = 0.0;
        let mut pending = vec![id];
        while let Some(node) = pending.pop() {
            if width > MAX_COORD {
                break;
            }
            if node != id
                && (!is_visible(self.document, self.styles, node)
                    || self.styles[node].position == crate::style::Position::Absolute)
            {
                continue;
            }
            if let NodeKind::Text(value) = &self.document.nodes[node].kind {
                let style = &self.styles[node];
                let cached = { self.intrinsic_text_widths.borrow()[node] };
                let measured = cached.unwrap_or_else(|| {
                    let measured = text::width(value.trim(), style.font_size, style.bold);
                    self.intrinsic_text_widths.borrow_mut()[node] = Some(measured);
                    measured
                });
                width += measured;
            }
            pending.extend(self.document.nodes[node].children.iter().copied());
        }
        width.min(MAX_COORD)
    }
}

fn list_markers(document: &Document, styles: &[ComputedStyle]) -> Vec<Option<String>> {
    let mut markers = vec![None; document.nodes.len()];
    for (id, node) in document.nodes.iter().enumerate() {
        let Some(element) = document
            .element(id)
            .filter(|element| matches!(element.tag.as_str(), "ul" | "ol"))
        else {
            continue;
        };
        let mut number = element
            .attribute("start")
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(1)
            .clamp(-1_000_000, 1_000_000);
        for &child in &node.children {
            if !document
                .element(child)
                .is_some_and(|element| element.tag == "li")
            {
                continue;
            }
            markers[child] = match styles[child].list_style_type {
                crate::style::ListStyleType::None => None,
                crate::style::ListStyleType::Disc => Some("•".into()),
                crate::style::ListStyleType::Decimal => Some(format!("{number}.")),
            };
            number = number.saturating_add(1);
        }
    }
    markers
}

pub fn layout(document: &Document, styles: &[ComputedStyle], viewport_width: f32) -> Scene {
    let images = vec![None; document.nodes.len()];
    layout_with_images(document, styles, &images, viewport_width)
}

pub fn layout_with_images(
    document: &Document,
    styles: &[ComputedStyle],
    images: &[Option<ImageSource>],
    viewport_width: f32,
) -> Scene {
    let viewport_width = if viewport_width.is_finite() {
        viewport_width.clamp(1.0, 16_384.0)
    } else {
        1.0
    };
    let list_markers = list_markers(document, styles);
    let mut builder = Builder {
        document,
        styles,
        images,
        list_markers: &list_markers,
        boxes: Vec::new(),
        primitives: Vec::new(),
        runs: Vec::new(),
        line_boxes: Vec::new(),
        last_flow_baseline: None,
        paint_groups: Vec::new(),
        inline_backgrounds: Vec::new(),
        current_group: None,
        absolute_override: None,
        anchors: Vec::new(),
        lines: 0,
        truncated: false,
        budget: std::rc::Rc::new(std::cell::Cell::new(MAX_ITEMS)),
        intrinsic_text_widths: Rc::new(RefCell::new(vec![None; document.nodes.len()])),
    };
    let flow_height = builder.children(0, 0.0, 0.0, viewport_width, None, 0);
    let normal_end = builder.primitives.len();
    builder.positioned(viewport_width);
    let painted_height = builder
        .boxes
        .iter()
        .map(|item| item.y + item.height)
        .fold(0.0, f32::max);
    let primitives = crate::stacking::order(
        builder.primitives,
        &builder.paint_groups,
        &builder.inline_backgrounds,
        styles,
        document,
        normal_end,
    );
    builder.truncated |= primitives.len() > MAX_ITEMS
        || builder.boxes.len() > MAX_ITEMS
        || builder.runs.len() > MAX_ITEMS
        || builder.line_boxes.len() > MAX_ITEMS;
    let mut primitives = primitives;
    primitives.truncate(MAX_ITEMS);
    builder.boxes.truncate(MAX_ITEMS);
    builder.runs.truncate(MAX_ITEMS);
    builder.line_boxes.truncate(MAX_ITEMS);
    // Inspectable runs are grouped by their final line and left-to-right visual
    // position, independently of the separate stacking/painting traversal.
    builder
        .runs
        .retain(|run| run.line < builder.line_boxes.len());
    builder
        .runs
        .sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.x.total_cmp(&b.x)));
    let mut cursor = 0;
    for (index, line) in builder.line_boxes.iter_mut().enumerate() {
        let start = cursor;
        while cursor < builder.runs.len() && builder.runs[cursor].line == index {
            builder.runs[cursor].visual_order = cursor - start;
            cursor += 1;
        }
        line.runs = start..cursor;
    }
    Scene {
        width: viewport_width,
        height: flow_height.max(painted_height).clamp(1.0, MAX_COORD),
        boxes: builder.boxes,
        primitives,
        runs: builder.runs,
        line_boxes: builder.line_boxes,
        truncated: builder.truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{css, html, style};

    #[test]
    fn wraps_text_and_respects_hidden_nodes() {
        let document = html::parse("<style>.hidden { display:none }</style><div>alpha beta gamma</div><p class='hidden'>gone</p>").unwrap();
        let styles = style::compute(&document, &css::parse(&document.stylesheets()));
        let scene = layout(&document, &styles, 110.0);
        let words: Vec<_> = scene
            .primitives
            .iter()
            .filter_map(|primitive| match primitive {
                Primitive::Text { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(words, ["alpha", " ", "beta", "gamma"]);
        assert!(scene.height > 20.0);
    }
}
