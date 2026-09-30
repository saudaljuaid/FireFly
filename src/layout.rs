use crate::dom::{Document, NodeId, NodeKind};
use crate::style::{
    BorderStyle, BoxSizing, Color, ComputedStyle, Display, Edges, Length, TextAlign, WhiteSpace,
    is_visible,
};
use crate::text;
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
                *x += dx;
                *y += dy;
            }
            Self::Text { x, baseline, .. } => {
                *x += dx;
                *baseline += dy;
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
}

fn edge_values(edges: Edges, base: f32) -> [f32; 4] {
    [
        edges.top.resolve(base).unwrap_or(0.0),
        edges.right.resolve(base).unwrap_or(0.0),
        edges.bottom.resolve(base).unwrap_or(0.0),
        edges.left.resolve(base).unwrap_or(0.0),
    ]
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
    Break,
    Atomic(NodeId),
    Block(NodeId),
}

struct Builder<'a> {
    document: &'a Document,
    styles: &'a [ComputedStyle],
    images: &'a [Option<ImageSource>],
    boxes: Vec<BoxGeometry>,
    primitives: Vec<Primitive>,
    lines: usize,
}

struct InlineFragment {
    id: NodeId,
    start: f32,
    end: f32,
}

enum Placement {
    Text {
        x: f32,
        content: String,
        width: f32,
        style: NodeId,
    },
    Atomic {
        x: f32,
        height: f32,
        primitives: Vec<Primitive>,
        boxes: Vec<BoxGeometry>,
    },
}

struct Line {
    x0: f32,
    x: f32,
    y: f32,
    width: f32,
    ascent: f32,
    descent: f32,
    placements: Vec<Placement>,
    fragments: Vec<InlineFragment>,
}

impl Line {
    fn new(x: f32, y: f32, width: f32) -> Self {
        Self {
            x0: x,
            x,
            y,
            width,
            ascent: 0.0,
            descent: 0.0,
            placements: Vec::new(),
            fragments: Vec::new(),
        }
    }

    fn has_content(&self) -> bool {
        !self.placements.is_empty() || self.x > self.x0
    }

    fn include(&mut self, active: &[NodeId], start: f32, end: f32) {
        for &id in active {
            if let Some(fragment) = self.fragments.iter_mut().find(|fragment| fragment.id == id) {
                fragment.start = fragment.start.min(start);
                fragment.end = fragment.end.max(end);
            } else {
                self.fragments.push(InlineFragment { id, start, end });
            }
        }
    }
}

impl Builder<'_> {
    fn flatten(&self, id: NodeId, items: &mut Vec<FlowItem>, depth: usize) {
        if depth > 256 || items.len() >= MAX_ITEMS || !is_visible(self.document, self.styles, id) {
            return;
        }
        match &self.document.nodes[id].kind {
            NodeKind::Text(text) => items.push(FlowItem::Text(id, text.clone())),
            NodeKind::Element(element) if element.tag == "br" => items.push(FlowItem::Break),
            NodeKind::Element(_) if self.styles[id].display == Display::Block => {
                items.push(FlowItem::Block(id))
            }
            NodeKind::Element(_) if self.styles[id].display == Display::InlineBlock => {
                items.push(FlowItem::Atomic(id))
            }
            NodeKind::Element(_) => {
                items.push(FlowItem::Start(id));
                for &child in &self.document.nodes[id].children {
                    self.flatten(child, items, depth + 1);
                }
                items.push(FlowItem::End(id));
            }
            _ => {}
        }
    }

    fn finish_line(&mut self, line: &mut Line, align: TextAlign, forced: bool) -> f32 {
        if !line.has_content() && !forced {
            return line.y;
        }
        self.lines += 1;
        let line_height = (line.ascent + line.descent).clamp(19.2, MAX_COORD);
        let available = (line.width - (line.x - line.x0)).max(0.0);
        let offset = match align {
            TextAlign::Left => 0.0,
            TextAlign::Center => available / 2.0,
            TextAlign::Right => available,
        };
        // Anonymous line boxes make mixed inline/block flow inspectable.
        self.boxes.push(BoxGeometry {
            node: None,
            x: line.x0,
            y: line.y,
            width: line.width,
            height: line_height,
            kind: BoxKind::Line,
        });
        for fragment in &line.fragments {
            let style = &self.styles[fragment.id];
            let width = (fragment.end - fragment.start).max(0.0);
            let geometry = BoxGeometry {
                node: Some(fragment.id),
                x: fragment.start + offset,
                y: line.y,
                width,
                height: line_height,
                kind: BoxKind::InlineFragment,
            };
            self.boxes.push(geometry);
            if style.background.is_some() || style.border_style != BorderStyle::None {
                let border = if style.border_style == BorderStyle::None {
                    [0.0; 4]
                } else {
                    edge_values(style.border_width, line.width)
                };
                self.primitives.push(Primitive::Box {
                    x: fragment.start + offset,
                    y: line.y,
                    width,
                    height: line_height,
                    background: style.background,
                    border_color: style.border_color,
                    border_width: border,
                    border_style: style.border_style,
                    radius: rounded(style, width, line_height),
                });
            }
        }
        for placement in std::mem::take(&mut line.placements) {
            match placement {
                Placement::Text {
                    x,
                    content,
                    width,
                    style,
                } => {
                    let style = &self.styles[style];
                    self.primitives.push(Primitive::Text {
                        x: x + offset,
                        baseline: line.y + line.ascent,
                        content,
                        width,
                        size: style.font_size,
                        color: style.color,
                        bold: style.bold,
                    });
                }
                Placement::Atomic {
                    x,
                    height,
                    mut primitives,
                    mut boxes,
                } => {
                    let dx = x + offset;
                    let dy = line.y + line.ascent - height;
                    for primitive in &mut primitives {
                        primitive.translate(dx, dy);
                    }
                    for geometry in &mut boxes {
                        geometry.x += dx;
                        geometry.y += dy;
                    }
                    self.primitives.extend(primitives);
                    self.boxes.extend(boxes);
                }
            }
        }
        (line.y + line_height).min(MAX_COORD)
    }

    fn push_text(
        &mut self,
        line: &mut Line,
        text: &str,
        id: NodeId,
        active: &[NodeId],
        wrap: bool,
    ) {
        if text.is_empty()
            || self.lines >= MAX_ITEMS
            || self.primitives.len() + line.placements.len() >= MAX_ITEMS
        {
            return;
        }
        let style = &self.styles[id];
        let measured = text::width(text, style.font_size, style.bold);
        if wrap && line.has_content() && line.x + measured > line.x0 + line.width {
            let y = self.finish_line(line, style.text_align, false);
            *line = Line::new(line.x0, y, line.width);
        }
        if wrap && measured > line.width && line.width > 0.0 {
            let mut chunk = String::new();
            let mut chunk_width = 0.0;
            for grapheme in text.graphemes(true) {
                if self.lines >= MAX_ITEMS
                    || self.primitives.len() + line.placements.len() >= MAX_ITEMS
                {
                    break;
                }
                let advance = text::width(grapheme, style.font_size, style.bold);
                if !chunk.is_empty() && line.x + chunk_width + advance > line.x0 + line.width {
                    self.place_chunk(line, std::mem::take(&mut chunk), chunk_width, id, active);
                    let y = self.finish_line(line, style.text_align, false);
                    *line = Line::new(line.x0, y, line.width);
                    chunk_width = 0.0;
                }
                chunk.push_str(grapheme);
                chunk_width += advance;
            }
            if !chunk.is_empty() {
                self.place_chunk(line, chunk, chunk_width, id, active);
            }
        } else {
            self.place_chunk(line, text.to_string(), measured, id, active);
        }
    }

    fn place_chunk(
        &self,
        line: &mut Line,
        content: String,
        width: f32,
        id: NodeId,
        active: &[NodeId],
    ) {
        let style = &self.styles[id];
        let leading = (style.line_height - style.font_size).max(0.0) / 2.0;
        line.ascent = line.ascent.max(style.font_size * 0.8 + leading);
        line.descent = line.descent.max(style.font_size * 0.2 + leading);
        let end = (line.x + width).min(MAX_COORD);
        line.include(active, line.x, end);
        line.placements.push(Placement::Text {
            x: line.x,
            content,
            width,
            style: id,
        });
        line.x = end;
    }

    fn text_item(
        &mut self,
        line: &mut Line,
        id: NodeId,
        source: &str,
        active: &[NodeId],
        pending_space: &mut bool,
    ) {
        let style = &self.styles[id];
        let white_space = style.white_space;
        let wrap = !matches!(white_space, WhiteSpace::NoWrap | WhiteSpace::Pre);
        if matches!(white_space, WhiteSpace::Pre | WhiteSpace::PreWrap) {
            for (index, segment) in source.split('\n').enumerate() {
                if index > 0 {
                    let y = self.finish_line(line, style.text_align, true);
                    *line = Line::new(line.x0, y, line.width);
                }
                if !segment.is_empty() {
                    self.push_text(line, segment, id, active, wrap);
                }
            }
            *pending_space = false;
            return;
        }
        let mut word = String::new();
        for character in source.chars().chain(std::iter::once('\0')) {
            if character.is_ascii_whitespace() || character == '\0' {
                if !word.is_empty() {
                    let content = std::mem::take(&mut word);
                    if *pending_space && line.has_content() {
                        let gap = text::width(" ", style.font_size, style.bold);
                        let word_width = text::width(&content, style.font_size, style.bold);
                        if wrap && line.x + gap + word_width > line.x0 + line.width {
                            let y = self.finish_line(line, style.text_align, false);
                            *line = Line::new(line.x0, y, line.width);
                        } else {
                            line.include(active, line.x, line.x + gap);
                            line.x += gap;
                        }
                    }
                    self.push_text(line, &content, id, active, wrap);
                    *pending_space = false;
                }
                if character == '\n' && white_space == WhiteSpace::PreLine {
                    let y = self.finish_line(line, style.text_align, true);
                    *line = Line::new(line.x0, y, line.width);
                    *pending_space = false;
                } else if character != '\0' {
                    *pending_space = true;
                }
            } else {
                word.push(character);
            }
            if self.lines >= MAX_ITEMS || self.primitives.len() + line.placements.len() >= MAX_ITEMS
            {
                break;
            }
        }
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
        if depth > 256 || self.primitives.len() >= MAX_ITEMS {
            return 0.0;
        }
        let mut items = Vec::new();
        for &child in &self.document.nodes[id].children {
            self.flatten(child, &mut items, depth + 1);
        }
        let mut line = Line::new(x, y, width);
        let mut run_start = y;
        let mut active = Vec::new();
        let mut pending_space = false;
        let align = self.styles[id].text_align;
        for item in items {
            if self.lines >= MAX_ITEMS || line.y >= MAX_COORD {
                break;
            }
            match item {
                FlowItem::Start(child) => {
                    let style = &self.styles[child];
                    let (padding, border) = inset(style, width);
                    let margin = edge_values(style.margin, width);
                    let before = (padding[3] + border[3] + margin[3]).max(0.0);
                    active.push(child);
                    line.include(&active, line.x, line.x + before);
                    line.x = (line.x + before).min(MAX_COORD);
                }
                FlowItem::End(child) => {
                    let style = &self.styles[child];
                    let (padding, border) = inset(style, width);
                    let margin = edge_values(style.margin, width);
                    let after = (padding[1] + border[1] + margin[1]).max(0.0);
                    line.include(&active, line.x, line.x + after);
                    line.x = (line.x + after).min(MAX_COORD);
                    if active.last() == Some(&child) {
                        active.pop();
                    }
                }
                FlowItem::Text(child, source) => {
                    self.text_item(&mut line, child, &source, &active, &mut pending_space)
                }
                FlowItem::Break => {
                    let next = self.finish_line(&mut line, align, true);
                    line = Line::new(x, next, width);
                    pending_space = false;
                }
                FlowItem::Atomic(child) => {
                    let mut nested = Builder {
                        document: self.document,
                        styles: self.styles,
                        images: self.images,
                        boxes: Vec::new(),
                        primitives: Vec::new(),
                        lines: 0,
                    };
                    let (atomic_width, atomic_height) =
                        nested.block(child, (0.0, 0.0), width, containing_height, depth + 1, true);
                    if line.has_content() && line.x + atomic_width > line.x0 + line.width {
                        let next = self.finish_line(&mut line, align, false);
                        line = Line::new(x, next, width);
                    }
                    line.include(&active, line.x, line.x + atomic_width);
                    line.ascent = line.ascent.max(atomic_height);
                    line.placements.push(Placement::Atomic {
                        x: line.x,
                        height: atomic_height,
                        primitives: nested.primitives,
                        boxes: nested.boxes,
                    });
                    line.x = (line.x + atomic_width).min(MAX_COORD);
                    pending_space = false;
                }
                FlowItem::Block(child) => {
                    let next = self.finish_line(&mut line, align, false);
                    if next > run_start {
                        self.boxes.push(BoxGeometry {
                            node: None,
                            x,
                            y: run_start,
                            width,
                            height: next - run_start,
                            kind: BoxKind::AnonymousBlock,
                        });
                    }
                    let (_, height) =
                        self.block(child, (x, next), width, containing_height, depth + 1, false);
                    line = Line::new(x, (next + height).min(MAX_COORD), width);
                    run_start = line.y;
                    pending_space = false;
                }
            }
        }
        let bottom = self.finish_line(&mut line, align, false);
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
            return (0.0, 0.0);
        }
        let style = &self.styles[id];
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
        let css_width = style
            .width
            .and_then(|value| value.resolve(available))
            .map(|specified| {
                if style.box_sizing == BoxSizing::BorderBox {
                    specified - horizontal_inset
                } else {
                    specified
                }
            });
        let css_height = style
            .height
            .and_then(|value| vertical_length(value, containing_height))
            .map(|specified| {
                if style.box_sizing == BoxSizing::BorderBox {
                    specified - vertical_inset
                } else {
                    specified
                }
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
        if let Some(min) = constraint(style.min_width) {
            content_width = content_width.max(min);
        }
        if let Some(max) = constraint(style.max_width) {
            content_width = content_width.min(max);
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
        let border_width = content_width + horizontal_inset;
        let content_x = outer_x + border[3] + padding[3];
        let content_y = outer_y + border[0] + padding[0];
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
        let mut content_height = css_height.or(html_height).unwrap_or_else(|| {
            if let Some(image) = image {
                image.height * content_width / image.width.max(1.0)
            } else if is_image {
                style.line_height
            } else {
                child_height
            }
        });
        if let Some(min) = style
            .min_height
            .and_then(|v| vertical_length(v, containing_height))
        {
            content_height = content_height.max(min);
        }
        if let Some(max) = style
            .max_height
            .and_then(|v| vertical_length(v, containing_height))
        {
            content_height = content_height.min(max);
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
            self.primitives.push(Primitive::Text {
                x: content_x,
                baseline: content_y + style.font_size,
                content: alt.to_string(),
                width: text::width(alt, style.font_size, style.bold),
                size: style.font_size,
                color: style.color,
                bold: style.bold,
            });
        }
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
                    x: outer_x + border[3],
                    y: outer_y + border[0],
                    width: (border_width - border[3] - border[1]).max(0.0),
                    height: (border_height - border[0] - border[2]).max(0.0),
                    radius: clip_radius,
                },
            );
            self.primitives.push(Primitive::ClipEnd);
        }
        let outer_height = (margins[0] + border_height + margins[2]).clamp(0.0, MAX_COORD);
        (border_width + margin_left + margin_right, outer_height)
    }

    fn intrinsic_width(&self, id: NodeId) -> f32 {
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
            if let NodeKind::Text(value) = &self.document.nodes[node].kind {
                let style = &self.styles[node];
                width += text::width(value.trim(), style.font_size, style.bold);
            }
            pending.extend(self.document.nodes[node].children.iter().copied());
        }
        width.min(MAX_COORD)
    }
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
    let mut builder = Builder {
        document,
        styles,
        images,
        boxes: Vec::new(),
        primitives: Vec::new(),
        lines: 0,
    };
    let flow_height = builder.children(0, 0.0, 0.0, viewport_width, None, 0);
    let painted_height = builder
        .boxes
        .iter()
        .map(|item| item.y + item.height)
        .fold(0.0, f32::max);
    Scene {
        width: viewport_width,
        height: flow_height.max(painted_height).clamp(1.0, MAX_COORD),
        boxes: builder.boxes,
        primitives: builder.primitives,
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
        assert_eq!(words, ["alpha", "beta", "gamma"]);
        assert!(scene.height > 20.0);
    }
}
