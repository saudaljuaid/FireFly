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

mod grid_layout;

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
    DecoratedBox {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        background: Option<Color>,
        border_color: Color,
        border_width: [f32; 4],
        border_style: BorderStyle,
        radius: [f32; 4],
        gradient: Option<crate::effects::LinearGradient>,
        shadows: Vec<crate::effects::BoxShadow>,
    },
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
            Self::Box { x, y, .. }
            | Self::DecoratedBox { x, y, .. }
            | Self::Image { x, y, .. }
            | Self::ClipStart { x, y, .. } => {
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
    length.resolve_indefinite(containing_height)
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

fn box_primitive(
    style: &ComputedStyle,
    geometry: (f32, f32, f32, f32),
    border_width: [f32; 4],
    radius: [f32; 4],
) -> Primitive {
    let (x, y, width, height) = geometry;
    if style.background_gradient.is_some() || !style.box_shadows.is_empty() {
        Primitive::DecoratedBox {
            x,
            y,
            width,
            height,
            background: style.background,
            border_color: style.border_color,
            border_width,
            border_style: style.border_style,
            radius,
            gradient: style.background_gradient.clone(),
            shadows: style.box_shadows.clone(),
        }
    } else {
        Primitive::Box {
            x,
            y,
            width,
            height,
            background: style.background,
            border_color: style.border_color,
            border_width,
            border_style: style.border_style,
            radius,
        }
    }
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
    intrinsic: Rc<crate::intrinsic::IntrinsicCache<'a>>,
    measure_only: bool,
    measure_budget: Rc<Cell<usize>>,
    measurements: Rc<RefCell<HashMap<MeasureKey, MeasureResult>>>,
    layout_override: Option<(NodeId, SizeOverride)>,
    layout_work: Rc<Cell<usize>>,
    viewport_height: Option<f32>,
    first_flow_baseline: Option<f32>,
}

#[derive(Clone, Copy, Default)]
struct SizeOverride {
    width: Option<f32>,
    height: Option<f32>,
    suppress_margins: bool,
    indefinite_height: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct MeasureKey {
    node: NodeId,
    width: u32,
    height: Option<u32>,
    forced_width: Option<u32>,
    forced_height: Option<u32>,
    indefinite_height: bool,
    children_only: bool,
}

#[derive(Clone, Copy, Default)]
struct MeasureResult {
    width: f32,
    height: f32,
    baseline: Option<f32>,
    truncated: bool,
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
            intrinsic: self.intrinsic.clone(),
            measure_only: self.measure_only,
            measure_budget: self.measure_budget.clone(),
            measurements: self.measurements.clone(),
            layout_override: None,
            layout_work: self.layout_work.clone(),
            viewport_height: self.viewport_height,
            first_flow_baseline: None,
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
            NodeKind::Element(_) if self.styles[id].display.is_block_level() => {
                items.push(FlowItem::Block(id))
            }
            NodeKind::Element(_) if self.styles[id].display.is_atomic() => {
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
        if self.measure_only {
            if self.first_flow_baseline.is_none() {
                self.first_flow_baseline = Some(baseline);
            }
            self.last_flow_baseline = Some(baseline);
            return (line.y + height).min(MAX_COORD);
        }
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
            if style.background.is_some()
                || style.background_gradient.is_some()
                || !style.box_shadows.is_empty()
                || style.border_style != BorderStyle::None
            {
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
                self.primitives.push(box_primitive(
                    style,
                    (x, y, width, fragment_height),
                    fragment_border,
                    radius,
                ));
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
        if self.first_flow_baseline.is_none() {
            self.first_flow_baseline = Some(baseline);
        }
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
        } else if self.styles[id].display.is_flex() || self.styles[id].display.is_grid() {
            nested.first_flow_baseline.unwrap_or(h)
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
                    let advances = text::grapheme_advances(run)
                        .into_iter()
                        .map(|(_, advance)| advance);
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
        height_space: crate::sizing::AxisSpace,
        depth: usize,
    ) -> f32 {
        let containing_height = height_space.percentage_basis;
        use crate::style::UnicodeBidi;
        if depth > 256 || self.primitives.len() >= MAX_ITEMS {
            self.truncated = true;
            return 0.0;
        }
        if self.styles[id].display.is_flex() {
            return self.flex_children(id, x, y, width, height_space, depth);
        }
        if self.styles[id].display.is_grid() {
            return self.grid_children(id, x, y, width, height_space, depth);
        }
        let mut items = Vec::new();
        if matches!(self.document.nodes[id].kind, NodeKind::Text(_)) {
            for node in self.intrinsic.anonymous_nodes(id) {
                self.flatten(node, &mut items, depth + 1, 0);
            }
        } else {
            for &child in &self.document.nodes[id].children {
                self.flatten(child, &mut items, depth + 1, 0);
            }
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
        let forced = self
            .layout_override
            .filter(|(node, _)| *node == id)
            .map(|(_, size)| size);
        let margins = if forced.is_some_and(|size| size.suppress_margins) {
            [0.0; 4]
        } else {
            edge_values(style.margin, available)
        };
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
        let html_width = if is_image && !style.width_declared {
            html_dimension("width")
        } else {
            None
        };
        let html_height = if is_image && !style.height_declared {
            html_dimension("height")
        } else {
            None
        };
        let css_width = forced.and_then(|size| size.width).or_else(|| {
            absolute_size
                .and_then(|size| size.content_width)
                .or_else(|| {
                    style.width.and_then(|value| {
                        self.resolved_width(id, value, available, horizontal_inset)
                    })
                })
        });
        let height_constraint = |value: Option<Length>| {
            value
                .and_then(|length| vertical_length(length, containing_height))
                .map(|value| crate::sizing::content_size(value, vertical_inset, style.box_sizing))
        };
        let minimum_height = height_constraint(style.min_height);
        let maximum_height = height_constraint(style.max_height);
        let html_height = html_height
            .map(|height| crate::sizing::constrain(height, minimum_height, maximum_height));
        let css_height = forced
            .and_then(|size| size.height)
            .or_else(|| {
                absolute_size
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
                    })
            })
            .map(|height| crate::sizing::constrain(height, minimum_height, maximum_height));
        let width_constraint = |value: Option<Length>| {
            value.and_then(|value| self.resolved_width(id, value, available, horizontal_inset))
        };
        let replaced = image.map(|image| {
            crate::sizing::replaced_size(
                (image.width, image.height),
                (css_width.or(html_width), css_height.or(html_height)),
                (width_constraint(style.min_width), minimum_height),
                (width_constraint(style.max_width), maximum_height),
            )
        });
        let mut content_width = replaced.map(|size| size.0).or(css_width).or(html_width);
        if content_width.is_none() && atomic {
            content_width = Some(
                self.intrinsic
                    .content(id)
                    .shrink_to_fit(available - horizontal_inset - margins[1] - margins[3]),
            );
        }
        let auto_width = content_width.is_none();
        let left = if forced.is_some_and(|size| size.suppress_margins)
            || style.margin.left.resolve(available).is_none()
        {
            0.0
        } else {
            margins[3]
        };
        let right = if forced.is_some_and(|size| size.suppress_margins)
            || style.margin.right.resolve(available).is_none()
        {
            0.0
        } else {
            margins[1]
        };
        let mut content_width =
            content_width.unwrap_or((available - left - right - horizontal_inset).max(0.0));
        let constraint = |value: Option<crate::style::Length>| {
            value.and_then(|v| self.resolved_width(id, v, available, horizontal_inset))
        };
        if let Some(max) = constraint(style.max_width) {
            content_width = content_width.min(max);
        }
        if let Some(min) = constraint(style.min_width) {
            content_width = content_width.max(min);
        }
        content_width = content_width.clamp(0.0, MAX_COORD);
        let remaining = (available - content_width - horizontal_inset - left - right).max(0.0);
        let (margin_left, margin_right) =
            if auto_width || forced.is_some_and(|size| size.suppress_margins) {
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
        // Auto-height formatting containers first measure their natural content.
        // A changed min/max used height then participates in track/cross sizing.
        // This dry pass is cached and cannot append final geometry or paint.
        let used_height = css_height.or(html_height).or_else(|| {
            if (style.display.is_flex() || style.display.is_grid())
                && (minimum_height.is_some() || maximum_height.is_some())
            {
                let natural = self.measure_children(id, content_width, depth).height;
                let constrained = crate::sizing::constrain(natural, minimum_height, maximum_height);
                if (natural - constrained).abs() > 0.0001 {
                    Some(constrained)
                } else {
                    None
                }
            } else {
                None
            }
        });
        let child_height = if is_image {
            0.0
        } else {
            self.children(
                id,
                content_x,
                content_y,
                content_width,
                crate::sizing::AxisSpace::new(
                    used_height,
                    if forced.is_some_and(|size| size.indefinite_height) {
                        None
                    } else {
                        css_height.or(html_height)
                    },
                ),
                depth + 1,
            )
        };
        let child_height = if self.list_markers[id].is_some() {
            child_height.max(style.line_height)
        } else {
            child_height
        };
        let mut content_height = used_height.unwrap_or_else(|| {
            if let Some(image) = image {
                image.height * content_width / image.width.max(1.0)
            } else if is_image {
                style.line_height
            } else {
                child_height
            }
        });
        if let Some(max) = maximum_height {
            content_height = content_height.min(max);
        }
        if let Some(min) = minimum_height {
            content_height = content_height.max(min);
        }
        content_height = content_height.clamp(0.0, MAX_COORD);
        let border_height = (content_height + vertical_inset).clamp(0.0, MAX_COORD);
        if self.measure_only {
            self.current_group = parent_group;
            return (
                (border_width + margin_left + margin_right).clamp(0.0, MAX_COORD),
                (margins[0] + border_height + margins[2]).clamp(0.0, MAX_COORD),
            );
        }
        self.boxes.push(BoxGeometry {
            node: Some(id),
            x: outer_x,
            y: outer_y,
            width: border_width,
            height: border_height,
            kind: BoxKind::Element,
        });
        let has_box_paint = style.background.is_some()
            || style.background_gradient.is_some()
            || !style.box_shadows.is_empty()
            || style.border_style != BorderStyle::None;
        if has_box_paint {
            self.primitives.insert(
                paint_index,
                box_primitive(
                    style,
                    (outer_x, outer_y, border_width, border_height),
                    border,
                    rounded(style, border_width, border_height),
                ),
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
                height: self.viewport_height,
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
            let intrinsic = self.intrinsic.content(id);
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
                if !style.width_declared
                    && style
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
                if !style.height_declared
                    && style
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
            let mut size =
                crate::position::absolute_size_with_intrinsic(sizing_style, containing, intrinsic);
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
}

impl Builder<'_> {
    fn resolved_width(
        &self,
        id: NodeId,
        length: Length,
        available: f32,
        inset: f32,
    ) -> Option<f32> {
        let intrinsic = if matches!(length, Length::MinContent | Length::MaxContent) {
            self.intrinsic.content(id)
        } else {
            crate::sizing::IntrinsicSizes::default()
        };
        crate::sizing::resolve_content_size(
            intrinsic,
            length,
            crate::sizing::AvailableSize::Definite(available),
            inset,
            self.styles[id].box_sizing,
        )
    }
    fn formatting_items(&mut self, id: NodeId) -> Vec<NodeId> {
        self.intrinsic.formatting_nodes(id)
    }

    fn measure_item(
        &mut self,
        id: NodeId,
        available: f32,
        height: Option<f32>,
        forced: SizeOverride,
        depth: usize,
    ) -> MeasureResult {
        let key = MeasureKey {
            node: id,
            width: available.to_bits(),
            height: height.map(f32::to_bits),
            forced_width: forced.width.map(f32::to_bits),
            forced_height: forced.height.map(f32::to_bits),
            indefinite_height: forced.indefinite_height,
            children_only: false,
        };
        if let Some(result) = self.measurements.borrow().get(&key).copied() {
            self.truncated |= result.truncated;
            return result;
        }
        if self.measure_budget.get() == 0
            || self.measurements.borrow().len() >= 16_384
            || depth > 256
        {
            self.truncated = true;
            return MeasureResult {
                truncated: true,
                ..MeasureResult::default()
            };
        }
        let mut dry = self.nested();
        dry.measure_only = true;
        dry.budget = self.measure_budget.clone();
        dry.layout_override = Some((id, forced));
        let (width, height) = dry.block(id, (0.0, 0.0), available, height, depth + 1, false);
        let baseline = if self.document.element(id).is_some_and(|e| e.tag == "img") {
            Some(height)
        } else {
            Some(dry.first_flow_baseline.unwrap_or(height))
        };
        let mut result = MeasureResult {
            width,
            height,
            baseline,
            truncated: dry.truncated,
        };
        debug_assert!(dry.primitives.is_empty(), "measurement must not paint");
        if self.measurements.borrow().len() < 16_384 {
            self.measurements.borrow_mut().insert(key, result);
        } else {
            result.truncated = true;
        }
        self.truncated |= result.truncated;
        result
    }

    fn measure_children(&mut self, id: NodeId, width: f32, depth: usize) -> MeasureResult {
        let key = MeasureKey {
            node: id,
            width: width.to_bits(),
            children_only: true,
            height: None,
            forced_width: None,
            forced_height: None,
            indefinite_height: false,
        };
        if let Some(result) = self.measurements.borrow().get(&key).copied() {
            self.truncated |= result.truncated;
            return result;
        }
        if self.measure_budget.get() == 0
            || self.measurements.borrow().len() >= 16_384
            || depth > 256
        {
            self.truncated = true;
            return MeasureResult {
                truncated: true,
                ..MeasureResult::default()
            };
        }
        let mut dry = self.nested();
        dry.measure_only = true;
        dry.budget = self.measure_budget.clone();
        let height = dry.children(
            id,
            0.0,
            0.0,
            width,
            crate::sizing::AxisSpace::new(None, None),
            depth + 1,
        );
        let mut result = MeasureResult {
            width,
            height,
            baseline: dry.first_flow_baseline,
            truncated: dry.truncated,
        };
        debug_assert!(dry.primitives.is_empty(), "measurement must not paint");
        if self.measurements.borrow().len() < 16_384 {
            self.measurements.borrow_mut().insert(key, result);
        } else {
            result.truncated = true;
        }
        self.truncated |= result.truncated;
        result
    }

    fn lay_out_item(
        &mut self,
        id: NodeId,
        origin: (f32, f32),
        available: f32,
        height: Option<f32>,
        forced: SizeOverride,
        depth: usize,
    ) {
        if self.measure_only {
            let measured = self.measure_item(id, available, height, forced, depth);
            if let Some(baseline) = measured.baseline {
                if self.first_flow_baseline.is_none() {
                    self.first_flow_baseline = Some(origin.1 + baseline);
                }
                self.last_flow_baseline = Some(origin.1 + baseline);
            }
            return;
        }
        let old = self.layout_override;
        self.layout_override = Some((id, forced));
        self.block(id, origin, available, height, depth + 1, false);
        self.layout_override = old;
    }

    fn flex_children(
        &mut self,
        id: NodeId,
        x: f32,
        y: f32,
        width: f32,
        height_space: crate::sizing::AxisSpace,
        depth: usize,
    ) -> f32 {
        use crate::flex::{
            Config, CrossMeasurement, Item, resolve_cross, resolve_main_with_budget,
        };
        use crate::sizing::{AvailableSize, MAX_SIZE, content_size};
        let height = height_space.available.definite();
        let percentage_height = height_space.percentage_basis;
        if self.layout_work.get() == 0 {
            self.truncated = true;
            return 0.0;
        }
        let nodes = self.formatting_items(id);
        if nodes.len() > self.layout_work.get() {
            self.truncated = true;
            self.layout_work.set(0);
            return 0.0;
        }
        self.layout_work.set(self.layout_work.get() - nodes.len());
        let style = &self.styles[id];
        let row = style.flex_direction.is_row();
        let (padding, border) = inset(style, width);
        let vertical_inset = padding[0] + padding[2] + border[0] + border[2];
        let height_limit = |value: Option<Length>| {
            value
                .and_then(|value| value.resolve_indefinite(None))
                .map(|value| content_size(value, vertical_inset, style.box_sizing))
        };
        let collection_limit = height_limit(style.max_height)
            .map(|maximum| maximum.max(height_limit(style.min_height).unwrap_or(0.0)));
        let config = Config {
            main_size: if row {
                AvailableSize::Definite(width)
            } else {
                AvailableSize::from_option(height)
            },
            collection_size: if row || percentage_height.is_some() {
                None
            } else {
                Some(AvailableSize::from_option(collection_limit))
            },
            cross_size: if row {
                AvailableSize::from_option(height)
            } else {
                AvailableSize::Definite(width)
            },
            direction: style.flex_direction,
            wrap: style.flex_wrap,
            rtl: style.direction == crate::style::Direction::Rtl,
            main_gap: if row {
                style.column_gap.resolve(width)
            } else {
                style.row_gap.resolve_indefinite(percentage_height)
            }
            .unwrap_or(0.0),
            cross_gap: if row {
                style.row_gap.resolve_indefinite(percentage_height)
            } else {
                style.column_gap.resolve(width)
            }
            .unwrap_or(0.0),
            justify_content: style.justify_content,
            align_items: style.align_items,
            align_content: style.align_content,
        };
        let mut inputs = Vec::new();
        for &node in &nodes {
            let s = &self.styles[node];
            let needs_intrinsic = !row
                || [s.width, s.min_width, s.max_width, Some(s.flex_basis)]
                    .into_iter()
                    .flatten()
                    .any(|v| matches!(v, Length::MinContent | Length::MaxContent))
                || (!s.overflow_hidden
                    && s.min_width
                        .and_then(|v| v.resolve_indefinite(Some(width)))
                        .is_none())
                || (matches!(s.flex_basis, Length::Auto)
                    && s.width
                        .and_then(|v| v.resolve_indefinite(Some(width)))
                        .is_none());
            let intrinsic = if needs_intrinsic {
                self.intrinsic.content(node)
            } else {
                crate::sizing::IntrinsicSizes::default()
            };
            let (p, b) = inset(s, width);
            let horizontal = p[1] + p[3] + b[1] + b[3];
            let vertical = p[0] + p[2] + b[0] + b[2];
            let main_inset = if row { horizontal } else { vertical };
            let cross_inset = if row { vertical } else { horizontal };
            let main_available = if row { Some(width) } else { percentage_height };
            let cross_available = if row { percentage_height } else { Some(width) };
            let resolve_main_size = |value: Length| {
                if row {
                    intrinsic.resolve(value, config.main_size)
                } else {
                    value.resolve_indefinite(main_available)
                }
            };
            let main_content = |value: Length| {
                resolve_main_size(value).map(|v| {
                    if matches!(value, Length::MinContent | Length::MaxContent) {
                        v
                    } else {
                        content_size(v, main_inset, s.box_sizing)
                    }
                })
            };
            let preferred_property = if row { s.width } else { s.height };
            let preferred = preferred_property.and_then(main_content);
            let preferred_base = preferred_property.and_then(resolve_main_size).map(|v| {
                if matches!(
                    preferred_property,
                    Some(Length::MinContent | Length::MaxContent)
                ) || s.box_sizing == BoxSizing::ContentBox
                {
                    v
                } else {
                    v - main_inset
                }
            });
            let natural = if row {
                intrinsic.max_content
            } else {
                let natural_width = s
                    .width
                    .and_then(|v| {
                        crate::sizing::resolve_content_size(
                            intrinsic,
                            v,
                            AvailableSize::Definite(width),
                            horizontal,
                            s.box_sizing,
                        )
                    })
                    .unwrap_or(
                        (width - horizontal)
                            .max(intrinsic.min_content)
                            .min(intrinsic.max_content),
                    );
                self.measure_item(
                    node,
                    width,
                    percentage_height,
                    SizeOverride {
                        width: Some(natural_width),
                        height: None,
                        suppress_margins: true,
                        indefinite_height: false,
                    },
                    depth,
                )
                .height
                    - vertical
            };
            let base = if matches!(s.flex_basis, Length::Auto) {
                preferred_base.unwrap_or(natural)
            } else {
                resolve_main_size(s.flex_basis)
                    .map(|v| {
                        if matches!(s.flex_basis, Length::MinContent | Length::MaxContent)
                            || s.box_sizing == BoxSizing::ContentBox
                        {
                            v
                        } else {
                            v - main_inset
                        }
                    })
                    .unwrap_or(natural)
            };
            let min_property = if row { s.min_width } else { s.min_height };
            let max_property = if row { s.max_width } else { s.max_height };
            let maximum = max_property.and_then(main_content);
            let minimum = min_property.and_then(main_content).unwrap_or_else(|| {
                if s.overflow_hidden {
                    0.0
                } else {
                    if row { intrinsic.min_content } else { natural }
                        .min(preferred.unwrap_or(MAX_SIZE))
                        .min(maximum.unwrap_or(MAX_SIZE))
                }
            });
            let cross_resolve = |value: Length| {
                if row {
                    value.resolve_indefinite(cross_available)
                } else {
                    intrinsic.resolve(value, config.cross_size)
                }
            };
            let cross_content = |value: Length| {
                cross_resolve(value).map(|v| {
                    if matches!(value, Length::MinContent | Length::MaxContent) {
                        v
                    } else {
                        content_size(v, cross_inset, s.box_sizing)
                    }
                })
            };
            let min_cross = if row { s.min_height } else { s.min_width }
                .and_then(cross_content)
                .unwrap_or(0.0);
            let max_cross = if row { s.max_height } else { s.max_width }.and_then(cross_content);
            let margin = |length: Length| {
                if matches!(length, Length::Auto) {
                    None
                } else {
                    Some(length.resolve(width).unwrap_or(0.0))
                }
            };
            inputs.push(Item {
                order: s.order,
                base_size: base,
                min_size: minimum,
                max_size: maximum,
                main_inset,
                cross_inset,
                main_margin: if row {
                    [margin(s.margin.left), margin(s.margin.right)]
                } else {
                    [margin(s.margin.top), margin(s.margin.bottom)]
                },
                cross_margin: if row {
                    [margin(s.margin.top), margin(s.margin.bottom)]
                } else {
                    [margin(s.margin.left), margin(s.margin.right)]
                },
                grow: s.flex_grow,
                shrink: s.flex_shrink,
                cross_auto: if row { s.height } else { s.width }
                    .and_then(cross_resolve)
                    .is_none(),
                min_cross,
                max_cross,
                align_self: s.align_self,
            });
        }
        let mut plan = resolve_main_with_budget(&config, &inputs, self.layout_work.get());
        let main_work = plan.work;
        // Cross sizing is a bounded linear pass. Reserve its visits before
        // recursively measuring children so suspended ancestors cannot perform
        // unpaid numeric work after a descendant exhausts the shared budget.
        let cross_reserve = plan.items.len() * 5;
        let remaining = self.layout_work.get();
        debug_assert!(main_work + cross_reserve <= remaining);
        self.layout_work
            .set(remaining.saturating_sub(main_work + cross_reserve));
        let mut cross = Vec::new();
        for (i, &node) in nodes.iter().take(plan.items.len()).enumerate() {
            let s = &self.styles[node];
            let sizes = if row {
                crate::sizing::IntrinsicSizes::default()
            } else {
                self.intrinsic.content(node)
            };
            let natural_width = sizes
                .max_content
                .min((width - inputs[i].cross_inset).max(0.0))
                .max(sizes.min_content);
            let forced = SizeOverride {
                width: if row {
                    Some(plan.items[i].main_size)
                } else {
                    s.width
                        .and_then(|v| {
                            crate::sizing::resolve_content_size(
                                sizes,
                                v,
                                crate::sizing::AvailableSize::Definite(width),
                                inputs[i].cross_inset,
                                s.box_sizing,
                            )
                        })
                        .or(Some(natural_width))
                },
                height: if row {
                    None
                } else {
                    Some(plan.items[i].main_size)
                },
                suppress_margins: true,
                indefinite_height: !row && !flex_main_height_is_definite(s, percentage_height),
            };
            let align = if s.align_self == crate::style::Alignment::Auto {
                style.align_items
            } else {
                s.align_self
            };
            let known_cross = if row {
                s.height
                    .and_then(|v| v.resolve_indefinite(percentage_height))
                    .map(|v| content_size(v, inputs[i].cross_inset, s.box_sizing))
            } else {
                forced.width
            };
            let measurement = if align != crate::style::Alignment::Baseline
                && let Some(cross) = known_cross
            {
                let border =
                    crate::sizing::constrain(cross, Some(inputs[i].min_cross), inputs[i].max_cross)
                        + inputs[i].cross_inset;
                if row {
                    MeasureResult {
                        height: border,
                        ..MeasureResult::default()
                    }
                } else {
                    MeasureResult {
                        width: border,
                        ..MeasureResult::default()
                    }
                }
            } else {
                self.measure_item(node, width, percentage_height, forced, depth)
            };
            cross.push(CrossMeasurement {
                border_size: if row {
                    measurement.height
                } else {
                    measurement.width
                },
                baseline: if row { measurement.baseline } else { None },
            });
        }
        resolve_cross(&config, &inputs, &cross, &mut plan);
        self.truncated |= plan.truncated;
        let cross_work = plan.work - main_work;
        debug_assert!(cross_work <= cross_reserve);
        self.layout_work
            .set(self.layout_work.get() + cross_reserve.saturating_sub(cross_work));
        for &i in &plan.order {
            let position = &plan.items[i];
            let (ix, iy, iw, ih) = if row {
                (
                    position.main_position,
                    position.cross_position,
                    position.main_size,
                    position.cross_size,
                )
            } else {
                (
                    position.cross_position,
                    position.main_position,
                    position.cross_size,
                    position.main_size,
                )
            };
            let s = &self.styles[nodes[i]];
            let align = if s.align_self == crate::style::Alignment::Auto {
                style.align_items
            } else {
                s.align_self
            };
            let definite_height = if row {
                s.height
                    .and_then(|value| value.resolve_indefinite(percentage_height))
                    .is_some()
                    || (inputs[i].cross_auto
                        && align == crate::style::Alignment::Stretch
                        && inputs[i].cross_margin.iter().all(Option::is_some))
            } else {
                flex_main_height_is_definite(s, percentage_height)
            };
            self.lay_out_item(
                nodes[i],
                (x + ix, y + iy),
                width,
                percentage_height,
                SizeOverride {
                    width: Some(iw),
                    height: Some(ih),
                    suppress_margins: true,
                    indefinite_height: !definite_height,
                },
                depth,
            );
        }
        if row { plan.cross_size } else { plan.main_size }
    }
}

/// Flexbox 1 §9.8: a definite main container or definite flex basis makes
/// post-flexing main sizes definite. An auto basis in an auto-height container
/// retains an unresolved percentage basis even when min-height gives it space.
fn flex_main_height_is_definite(style: &ComputedStyle, containing: Option<f32>) -> bool {
    containing.is_some()
        || if matches!(style.flex_basis, Length::Auto) {
            style
                .height
                .and_then(|value| value.resolve_indefinite(containing))
                .is_some()
        } else {
            style.flex_basis.resolve_indefinite(containing).is_some()
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
    layout_with_images_and_viewport(
        document,
        styles,
        images,
        crate::values::Viewport {
            width: viewport_width,
            height: None,
        },
    )
}

pub fn layout_with_images_and_viewport(
    document: &Document,
    styles: &[ComputedStyle],
    images: &[Option<ImageSource>],
    viewport: crate::values::Viewport,
) -> Scene {
    let viewport_width = viewport.width;
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
        intrinsic: Rc::new(crate::intrinsic::IntrinsicCache::new(
            document, styles, images,
        )),
        measure_only: false,
        measure_budget: Rc::new(Cell::new(600_000)),
        measurements: Rc::new(RefCell::new(HashMap::new())),
        layout_override: None,
        layout_work: Rc::new(Cell::new(8_000_000)),
        viewport_height: viewport
            .height
            .filter(|height| height.is_finite())
            .map(|height| height.clamp(1.0, 16_384.0)),
        first_flow_baseline: None,
    };
    let flow_height = builder.children(
        0,
        0.0,
        0.0,
        viewport_width,
        crate::sizing::AxisSpace::new(builder.viewport_height, builder.viewport_height),
        0,
    );
    let normal_end = builder.primitives.len();
    builder.positioned(viewport_width);
    let mut painted_height = builder
        .boxes
        .iter()
        .map(|item| item.y + item.height)
        .fold(0.0, f32::max);
    let mut clip_bottoms: Vec<f32> = Vec::new();
    for primitive in &builder.primitives {
        match primitive {
            Primitive::ClipStart { y, height, .. } => {
                let bottom = y + height;
                clip_bottoms.push(
                    clip_bottoms
                        .last()
                        .copied()
                        .map_or(bottom, |old| old.min(bottom)),
                );
            }
            Primitive::ClipEnd => {
                clip_bottoms.pop();
            }
            Primitive::DecoratedBox {
                y, height, shadows, ..
            } => {
                for shadow in shadows {
                    let bottom = y + height + shadow.offset_y + shadow.spread + shadow.blur * 1.5;
                    painted_height = painted_height.max(
                        clip_bottoms
                            .last()
                            .copied()
                            .map_or(bottom, |clip| bottom.min(clip)),
                    );
                }
            }
            _ => {}
        }
    }
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
        truncated: builder.truncated || builder.intrinsic.truncated.get(),
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
