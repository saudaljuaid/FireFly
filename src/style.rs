use crate::css::{self, Declaration, Stylesheet};
use crate::dom::{Document, NodeId, NodeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    Block,
    Inline,
    InlineBlock,
    Flex,
    InlineFlex,
    Grid,
    InlineGrid,
    None,
}

impl Display {
    pub fn is_block_level(self) -> bool {
        matches!(self, Self::Block | Self::Flex | Self::Grid)
    }
    pub fn is_atomic(self) -> bool {
        matches!(
            self,
            Self::InlineBlock | Self::InlineFlex | Self::InlineGrid
        )
    }
    pub fn is_flex(self) -> bool {
        matches!(self, Self::Flex | Self::InlineFlex)
    }
    pub fn is_grid(self) -> bool {
        matches!(self, Self::Grid | Self::InlineGrid)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

impl Color {
    pub const BLACK: Self = Self(0, 0, 0, 255);
    pub const WHITE: Self = Self(255, 255, 255, 255);

    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if let Some(hex) = value.strip_prefix('#') {
            if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return None;
            }
            return match hex.len() {
                3 => {
                    let mut digits = hex.chars();
                    let r = digits.next()?.to_digit(16)? as u8 * 17;
                    let g = digits.next()?.to_digit(16)? as u8 * 17;
                    let b = digits.next()?.to_digit(16)? as u8 * 17;
                    Some(Self(r, g, b, 255))
                }
                4 => {
                    let mut digits = hex.chars();
                    let values: Vec<_> = (0..4)
                        .map(|_| digits.next().unwrap().to_digit(16).unwrap() as u8 * 17)
                        .collect();
                    Some(Self(values[0], values[1], values[2], values[3]))
                }
                6 => Some(Self(
                    u8::from_str_radix(&hex[0..2], 16).ok()?,
                    u8::from_str_radix(&hex[2..4], 16).ok()?,
                    u8::from_str_radix(&hex[4..6], 16).ok()?,
                    255,
                )),
                8 => Some(Self(
                    u8::from_str_radix(&hex[0..2], 16).ok()?,
                    u8::from_str_radix(&hex[2..4], 16).ok()?,
                    u8::from_str_radix(&hex[4..6], 16).ok()?,
                    u8::from_str_radix(&hex[6..8], 16).ok()?,
                )),
                _ => None,
            };
        }
        let lower = value.to_ascii_lowercase();
        if let Some(args) = lower
            .strip_prefix("rgb(")
            .and_then(|s| s.strip_suffix(')'))
            .or_else(|| {
                lower
                    .strip_prefix("rgba(")
                    .and_then(|s| s.strip_suffix(')'))
            })
        {
            let channels: Vec<_> = args.split(',').map(str::trim).collect();
            if matches!(channels.len(), 3 | 4) {
                let channel = |s: &str| -> Option<u8> {
                    if let Some(percent) = s.strip_suffix('%') {
                        let value: f32 = percent.parse().ok()?;
                        (value.is_finite() && (0.0..=100.0).contains(&value))
                            .then_some((value * 2.55).round() as u8)
                    } else {
                        s.parse::<u8>().ok()
                    }
                };
                let alpha = if channels.len() == 4 {
                    let value: f32 = channels[3].parse().ok()?;
                    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                        return None;
                    }
                    (value * 255.0).round() as u8
                } else {
                    255
                };
                return Some(Self(
                    channel(channels[0])?,
                    channel(channels[1])?,
                    channel(channels[2])?,
                    alpha,
                ));
            }
        }
        match lower.as_str() {
            "black" => Some(Self::BLACK),
            "white" => Some(Self::WHITE),
            "transparent" => Some(Self(0, 0, 0, 0)),
            "red" => Some(Self(255, 0, 0, 255)),
            "green" => Some(Self(0, 128, 0, 255)),
            "blue" => Some(Self(0, 0, 255, 255)),
            "navy" => Some(Self(0, 0, 128, 255)),
            "teal" => Some(Self(0, 128, 128, 255)),
            "gray" | "grey" => Some(Self(128, 128, 128, 255)),
            "silver" => Some(Self(192, 192, 192, 255)),
            "yellow" => Some(Self(255, 255, 0, 255)),
            "orange" => Some(Self(255, 165, 0, 255)),
            "purple" => Some(Self(128, 0, 128, 255)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    Px(f32),
    Percent(f32),
    Calc {
        px: f32,
        percent: f32,
        percentage: bool,
        nonnegative: bool,
    },
    MinContent,
    MaxContent,
    Auto,
}

impl Default for Length {
    fn default() -> Self {
        Self::Px(0.0)
    }
}

impl Length {
    pub fn resolve(self, base: f32) -> Option<f32> {
        self.resolve_indefinite(Some(base))
    }
    pub fn resolve_indefinite(self, base: Option<f32>) -> Option<f32> {
        match self {
            Self::Px(value) => Some(value),
            Self::Percent(value) => base.map(|base| base * value / 100.0),
            Self::Calc {
                px,
                percent,
                percentage,
                nonnegative,
            } => {
                if percentage && base.is_none() {
                    return None;
                }
                let value = px + base.unwrap_or(0.0) * percent / 100.0;
                Some(if nonnegative { value.max(0.0) } else { value })
            }
            Self::Auto | Self::MinContent | Self::MaxContent => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Auto,
    Start,
    End,
    FlexStart,
    FlexEnd,
    Center,
    Stretch,
    Baseline,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl Alignment {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "auto" => Self::Auto,
            "start" => Self::Start,
            "end" => Self::End,
            "flex-start" => Self::FlexStart,
            "flex-end" => Self::FlexEnd,
            "center" => Self::Center,
            "stretch" | "normal" => Self::Stretch,
            "baseline" | "first baseline" => Self::Baseline,
            "space-between" => Self::SpaceBetween,
            "space-around" => Self::SpaceAround,
            "space-evenly" => Self::SpaceEvenly,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxSizing {
    ContentBox,
    BorderBox,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyle {
    None,
    Solid,
    Dashed,
    Dotted,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    Start,
    End,
}
impl TextAlign {
    pub fn physical(self, direction: Direction) -> Self {
        match (self, direction) {
            (Self::Start, Direction::Ltr) | (Self::End, Direction::Rtl) => Self::Left,
            (Self::Start, Direction::Rtl) | (Self::End, Direction::Ltr) => Self::Right,
            _ => self,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnicodeBidi {
    Normal,
    Embed,
    Isolate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowWrap {
    Normal,
    Anywhere,
    BreakWord,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Static,
    Relative,
    Absolute,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhiteSpace {
    Normal,
    NoWrap,
    Pre,
    PreWrap,
    PreLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListStyleType {
    Disc,
    Decimal,
    None,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Edges {
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
    pub left: Length,
}

impl Edges {
    fn shorthand(values: &[Length]) -> Option<Self> {
        let edge = match values {
            [all] => Self {
                top: *all,
                right: *all,
                bottom: *all,
                left: *all,
            },
            [vertical, horizontal] => Self {
                top: *vertical,
                right: *horizontal,
                bottom: *vertical,
                left: *horizontal,
            },
            [top, horizontal, bottom] => Self {
                top: *top,
                right: *horizontal,
                bottom: *bottom,
                left: *horizontal,
            },
            [top, right, bottom, left] => Self {
                top: *top,
                right: *right,
                bottom: *bottom,
                left: *left,
            },
            _ => return None,
        };
        Some(edge)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    pub display: Display,
    pub flex_direction: crate::flex::FlexDirection,
    pub flex_wrap: crate::flex::FlexWrap,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Length,
    pub order: i32,
    pub row_gap: Length,
    pub column_gap: Length,
    pub justify_content: Alignment,
    pub align_content: Alignment,
    pub align_items: Alignment,
    pub align_self: Alignment,
    pub justify_items: Alignment,
    pub justify_self: Alignment,
    pub grid_template_columns: Vec<crate::grid::Track>,
    pub grid_template_rows: Vec<crate::grid::Track>,
    pub grid_auto_columns: Vec<crate::grid::Track>,
    pub grid_auto_rows: Vec<crate::grid::Track>,
    pub grid_auto_flow: crate::grid::GridAutoFlow,
    pub grid_column: crate::grid::GridLinePair,
    pub grid_row: crate::grid::GridLinePair,
    pub position: Position,
    pub insets: Edges,
    pub z_index: Option<i32>,
    pub color: Color,
    pub background: Option<Color>,
    pub background_gradient: Option<crate::effects::LinearGradient>,
    pub box_shadows: Vec<crate::effects::BoxShadow>,
    pub font_size: f32,
    pub bold: bool,
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub width_declared: bool,
    pub height_declared: bool,
    pub min_width: Option<Length>,
    pub max_width: Option<Length>,
    pub min_height: Option<Length>,
    pub max_height: Option<Length>,
    pub box_sizing: BoxSizing,
    pub margin: Edges,
    pub padding: Edges,
    pub border_width: Edges,
    pub border_style: BorderStyle,
    pub border_color: Color,
    border_color_explicit: bool,
    pub border_radius: Edges,
    pub line_height: f32,
    /// Unitless/normal values inherit their multiplier; explicit lengths inherit pixels.
    pub line_height_factor: Option<f32>,
    pub text_align: TextAlign,
    pub white_space: WhiteSpace,
    pub list_style_type: ListStyleType,
    pub direction: Direction,
    pub unicode_bidi: UnicodeBidi,
    pub overflow_wrap: OverflowWrap,
    pub overflow_hidden: bool,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display: Display::Inline,
            flex_direction: crate::flex::FlexDirection::Row,
            flex_wrap: crate::flex::FlexWrap::NoWrap,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Length::Auto,
            order: 0,
            row_gap: Length::Px(0.0),
            column_gap: Length::Px(0.0),
            justify_content: Alignment::Stretch,
            align_content: Alignment::Stretch,
            align_items: Alignment::Stretch,
            align_self: Alignment::Auto,
            justify_items: Alignment::Stretch,
            justify_self: Alignment::Auto,
            grid_template_columns: Vec::new(),
            grid_template_rows: Vec::new(),
            grid_auto_columns: Vec::new(),
            grid_auto_rows: Vec::new(),
            grid_auto_flow: crate::grid::GridAutoFlow::Row,
            grid_column: crate::grid::GridLinePair::default(),
            grid_row: crate::grid::GridLinePair::default(),
            position: Position::Static,
            insets: Edges {
                top: Length::Auto,
                right: Length::Auto,
                bottom: Length::Auto,
                left: Length::Auto,
            },
            z_index: None,
            color: Color::BLACK,
            background: None,
            background_gradient: None,
            box_shadows: Vec::new(),
            font_size: 16.0,
            bold: false,
            width: None,
            height: None,
            width_declared: false,
            height_declared: false,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            box_sizing: BoxSizing::ContentBox,
            margin: Edges::default(),
            padding: Edges::default(),
            border_width: Edges::default(),
            border_style: BorderStyle::None,
            border_color: Color::BLACK,
            border_color_explicit: false,
            border_radius: Edges::default(),
            line_height: 19.2,
            line_height_factor: Some(1.2),
            text_align: TextAlign::Start,
            white_space: WhiteSpace::Normal,
            list_style_type: ListStyleType::Disc,
            direction: Direction::Ltr,
            unicode_bidi: UnicodeBidi::Normal,
            overflow_wrap: OverflowWrap::BreakWord,
            overflow_hidden: false,
        }
    }
}

fn length(
    value: &str,
    em: f32,
    rem: f32,
    viewport: crate::values::Viewport,
    allow_auto: bool,
    allow_negative: bool,
) -> Option<Length> {
    crate::values::parse_length(
        value,
        &crate::values::LengthContext {
            em,
            rem,
            viewport_width: viewport.width,
            viewport_height: viewport.height,
        },
        allow_auto,
        allow_negative,
    )
}

fn sizing_length(
    value: &str,
    parse: impl Fn(&str, bool, bool) -> Option<Length>,
) -> Option<Length> {
    match value {
        "min-content" => Some(Length::MinContent),
        "max-content" => Some(Length::MaxContent),
        _ => parse(value, true, false),
    }
}

fn flex_factor(value: &str) -> Option<f32> {
    value
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite() && (0.0..=16_384.0).contains(value))
}

fn flex_shorthand(
    value: &str,
    parse: impl Fn(&str, bool, bool) -> Option<Length>,
) -> Option<(f32, f32, Length)> {
    match value {
        "none" => return Some((0.0, 0.0, Length::Auto)),
        "auto" => return Some((1.0, 1.0, Length::Auto)),
        "initial" => return Some((0.0, 1.0, Length::Auto)),
        _ => {}
    }
    let parts = crate::values::components(value)?;
    let basis = |value| {
        if value == "content" {
            Some(Length::MaxContent)
        } else {
            sizing_length(value, &parse)
        }
    };
    match parts.as_slice() {
        [a] => flex_factor(a)
            .map(|grow| (grow, 1.0, Length::Percent(0.0)))
            .or_else(|| basis(a).map(|basis| (1.0, 1.0, basis))),
        [a, b] => {
            let grow = flex_factor(a)?;
            flex_factor(b)
                .map(|shrink| (grow, shrink, Length::Percent(0.0)))
                .or_else(|| basis(b).map(|basis| (grow, 1.0, basis)))
        }
        [a, b, c] => Some((flex_factor(a)?, flex_factor(b)?, basis(c)?)),
        _ => None,
    }
}

fn apply_edges(
    edges: &mut Edges,
    property: &str,
    value: &str,
    prefix: &str,
    units: (f32, f32, crate::values::Viewport),
    allowances: (bool, bool),
) {
    let (em, rem, viewport) = units;
    let (allow_auto, allow_negative) = allowances;
    if property == prefix {
        let numbers: Option<Vec<_>> = crate::values::components(value)
            .unwrap_or_default()
            .into_iter()
            .map(|part| length(part, em, rem, viewport, allow_auto, allow_negative))
            .collect();
        if let Some(parsed) = numbers.and_then(|numbers| Edges::shorthand(&numbers)) {
            *edges = parsed;
        }
        return;
    }
    let Some(value) = length(value, em, rem, viewport, allow_auto, allow_negative) else {
        return;
    };
    match property.strip_prefix(prefix) {
        Some("-top") => edges.top = value,
        Some("-right") => edges.right = value,
        Some("-bottom") => edges.bottom = value,
        Some("-left") => edges.left = value,
        _ => {}
    }
}

impl ComputedStyle {
    fn inherit(parent: &Self) -> Self {
        Self {
            color: parent.color,
            font_size: parent.font_size,
            bold: parent.bold,
            line_height: parent.line_height,
            line_height_factor: parent.line_height_factor,
            text_align: parent.text_align,
            white_space: parent.white_space,
            list_style_type: parent.list_style_type,
            direction: parent.direction,
            overflow_wrap: parent.overflow_wrap,
            ..Self::default()
        }
    }

    fn apply(
        &mut self,
        declaration: &Declaration,
        parent_font: f32,
        root_font: f32,
        viewport: crate::values::Viewport,
    ) {
        let lower = declaration.value.to_ascii_lowercase();
        let value = lower.as_str();
        let font_size = self.font_size;
        let unit = |part: &str, auto: bool, negative: bool| {
            length(part, font_size, root_font, viewport, auto, negative)
        };
        match declaration.name.as_str() {
            "flex-direction" => match value {
                "row" => self.flex_direction = crate::flex::FlexDirection::Row,
                "row-reverse" => self.flex_direction = crate::flex::FlexDirection::RowReverse,
                "column" => self.flex_direction = crate::flex::FlexDirection::Column,
                "column-reverse" => self.flex_direction = crate::flex::FlexDirection::ColumnReverse,
                _ => {}
            },
            "flex-wrap" => match value {
                "nowrap" => self.flex_wrap = crate::flex::FlexWrap::NoWrap,
                "wrap" => self.flex_wrap = crate::flex::FlexWrap::Wrap,
                "wrap-reverse" => self.flex_wrap = crate::flex::FlexWrap::WrapReverse,
                _ => {}
            },
            "flex-grow" | "flex-shrink" => {
                if let Some(factor) = flex_factor(value) {
                    if declaration.name == "flex-grow" {
                        self.flex_grow = factor;
                    } else {
                        self.flex_shrink = factor;
                    }
                }
            }
            "flex-basis" => {
                if let Some(basis) = if value == "content" {
                    Some(Length::MaxContent)
                } else {
                    sizing_length(value, unit)
                } {
                    self.flex_basis = basis;
                }
            }
            "flex" => {
                if let Some((grow, shrink, basis)) = flex_shorthand(value, unit) {
                    self.flex_grow = grow;
                    self.flex_shrink = shrink;
                    self.flex_basis = basis;
                }
            }
            "order" => {
                if let Ok(order) = value.parse::<i32>()
                    && (-32_768..=32_767).contains(&order)
                {
                    self.order = order;
                }
            }
            "gap" => {
                if let Some(parts) = crate::values::components(value)
                    && matches!(parts.len(), 1 | 2)
                {
                    let gap = |v| {
                        if v == "normal" {
                            Some(Length::Px(0.0))
                        } else {
                            unit(v, false, false)
                        }
                    };
                    if let (Some(row), Some(column)) = (
                        gap(parts[0]),
                        gap(parts.get(1).copied().unwrap_or(parts[0])),
                    ) {
                        self.row_gap = row;
                        self.column_gap = column;
                    }
                }
            }
            "row-gap" | "column-gap" => {
                if let Some(gap) = if value == "normal" {
                    Some(Length::Px(0.0))
                } else {
                    unit(value, false, false)
                } {
                    if declaration.name == "row-gap" {
                        self.row_gap = gap;
                    } else {
                        self.column_gap = gap;
                    }
                }
            }
            "justify-content" | "align-content" | "align-items" | "align-self"
            | "justify-items" | "justify-self" => {
                if let Some(align) = Alignment::parse(value) {
                    let content = declaration.name.ends_with("content");
                    let distribution = matches!(
                        align,
                        Alignment::SpaceBetween | Alignment::SpaceAround | Alignment::SpaceEvenly
                    );
                    let auto = align == Alignment::Auto;
                    if (content && !auto && align != Alignment::Baseline)
                        || (!content
                            && !distribution
                            && (!auto || declaration.name.ends_with("self")))
                    {
                        match declaration.name.as_str() {
                            "justify-content" => self.justify_content = align,
                            "align-content" => self.align_content = align,
                            "align-items" => self.align_items = align,
                            "align-self" => self.align_self = align,
                            "justify-items" => self.justify_items = align,
                            _ => self.justify_self = align,
                        }
                    }
                }
            }
            "grid-template-columns"
            | "grid-template-rows"
            | "grid-auto-columns"
            | "grid-auto-rows" => {
                if let Some(tracks) =
                    crate::grid::parse_tracks(value, |part| unit(part, false, false))
                {
                    match declaration.name.as_str() {
                        "grid-template-columns" => self.grid_template_columns = tracks,
                        "grid-template-rows" => self.grid_template_rows = tracks,
                        "grid-auto-columns" => self.grid_auto_columns = tracks,
                        _ => self.grid_auto_rows = tracks,
                    }
                }
            }
            "grid-auto-flow" => match value {
                "row" => self.grid_auto_flow = crate::grid::GridAutoFlow::Row,
                "column" => self.grid_auto_flow = crate::grid::GridAutoFlow::Column,
                _ => {}
            },
            "grid-column" | "grid-row" => {
                if let Some(pair) = crate::grid::parse_placement(value) {
                    if declaration.name == "grid-column" {
                        self.grid_column = pair;
                    } else {
                        self.grid_row = pair;
                    }
                }
            }
            "grid-column-start" | "grid-column-end" | "grid-row-start" | "grid-row-end" => {
                if let Some(line) = crate::grid::parse_line(value) {
                    match declaration.name.as_str() {
                        "grid-column-start" => self.grid_column.start = line,
                        "grid-column-end" => self.grid_column.end = line,
                        "grid-row-start" => self.grid_row.start = line,
                        _ => self.grid_row.end = line,
                    }
                }
            }
            "list-style-type" | "list-style" => match value {
                "disc" => self.list_style_type = ListStyleType::Disc,
                "decimal" => self.list_style_type = ListStyleType::Decimal,
                "none" => self.list_style_type = ListStyleType::None,
                _ => {}
            },
            "display" => match value {
                "block" => self.display = Display::Block,
                "inline" => self.display = Display::Inline,
                "inline-block" => self.display = Display::InlineBlock,
                "flex" => self.display = Display::Flex,
                "inline-flex" => self.display = Display::InlineFlex,
                "grid" => self.display = Display::Grid,
                "inline-grid" => self.display = Display::InlineGrid,
                "none" => self.display = Display::None,
                _ => {}
            },
            "position" => match value {
                "static" => self.position = Position::Static,
                "relative" => self.position = Position::Relative,
                "absolute" => self.position = Position::Absolute,
                _ => {}
            },
            "top" | "right" | "bottom" | "left" => {
                if let Some(offset) = unit(value, true, true) {
                    match declaration.name.as_str() {
                        "top" => self.insets.top = offset,
                        "right" => self.insets.right = offset,
                        "bottom" => self.insets.bottom = offset,
                        _ => self.insets.left = offset,
                    }
                }
            }
            "inset" => {
                let offsets: Option<Vec<_>> = crate::values::components(value).and_then(|parts| {
                    parts
                        .into_iter()
                        .map(|part| unit(part, true, true))
                        .collect()
                });
                if let Some(offsets) = offsets.and_then(|values| Edges::shorthand(&values)) {
                    self.insets = offsets;
                }
            }
            "z-index" => {
                if value == "auto" {
                    self.z_index = None;
                } else if let Ok(index) = value.parse::<i32>()
                    && (-32_768..=32_767).contains(&index)
                {
                    self.z_index = Some(index);
                }
            }
            "direction" => match value {
                "ltr" => self.direction = Direction::Ltr,
                "rtl" => self.direction = Direction::Rtl,
                _ => {}
            },
            "unicode-bidi" => match value {
                "normal" => self.unicode_bidi = UnicodeBidi::Normal,
                "embed" => self.unicode_bidi = UnicodeBidi::Embed,
                "isolate" => self.unicode_bidi = UnicodeBidi::Isolate,
                _ => {}
            },
            "overflow-wrap" => match value {
                "normal" => self.overflow_wrap = OverflowWrap::Normal,
                "anywhere" => self.overflow_wrap = OverflowWrap::Anywhere,
                "break-word" => self.overflow_wrap = OverflowWrap::BreakWord,
                _ => {}
            },
            "color" => {
                if let Some(color) = Color::parse(value) {
                    self.color = color;
                }
            }
            "background" | "background-color" => {
                if let Some(color) = Color::parse(value) {
                    self.background = Some(color);
                    if declaration.name == "background" {
                        self.background_gradient = None;
                    }
                } else if declaration.name == "background" {
                    if value == "none" {
                        self.background = None;
                        self.background_gradient = None;
                    } else if let Some(gradient) =
                        crate::effects::parse_gradient(value, &mut |part| unit(part, false, true))
                    {
                        self.background = None;
                        self.background_gradient = Some(gradient);
                    }
                }
            }
            "background-image" => {
                if value == "none" {
                    self.background_gradient = None;
                } else if let Some(gradient) =
                    crate::effects::parse_gradient(value, &mut |part| unit(part, false, true))
                {
                    self.background_gradient = Some(gradient);
                }
            }
            "box-shadow" => {
                if let Some(shadows) =
                    crate::effects::parse_shadows(value, self.color, &mut |part| {
                        unit(part, false, true)
                    })
                {
                    self.box_shadows = shadows;
                }
            }
            "font-size" => {
                if let Some(size) = length(value, parent_font, root_font, viewport, false, false)
                    .and_then(|length| length.resolve(parent_font))
                    .filter(|size| (1.0..=1024.0).contains(size))
                {
                    self.font_size = size;
                    if let Some(factor) = self.line_height_factor {
                        self.line_height = size * factor;
                    }
                }
            }
            "font-weight" => match value {
                "bold" | "700" | "800" | "900" => self.bold = true,
                "normal" | "400" => self.bold = false,
                _ => {}
            },
            "width" => {
                if value == "auto" {
                    self.width = None;
                    self.width_declared = true;
                } else if let Some(size) = sizing_length(value, unit) {
                    self.width = Some(size);
                    self.width_declared = true;
                }
            }
            "height" => {
                if value == "auto" {
                    self.height = None;
                    self.height_declared = true;
                } else if let Some(size) = sizing_length(value, unit) {
                    self.height = Some(size);
                    self.height_declared = true;
                }
            }
            "min-width" => {
                if let Some(size) = sizing_length(value, unit) {
                    self.min_width = Some(size);
                }
            }
            "max-width" => {
                if value == "none" {
                    self.max_width = None;
                } else if let Some(size) = sizing_length(value, unit) {
                    self.max_width = Some(size);
                }
            }
            "min-height" => {
                if let Some(size) = sizing_length(value, unit) {
                    self.min_height = Some(size);
                }
            }
            "max-height" => {
                if value == "none" {
                    self.max_height = None;
                } else if let Some(size) = sizing_length(value, unit) {
                    self.max_height = Some(size);
                }
            }
            "box-sizing" => match value {
                "content-box" => self.box_sizing = BoxSizing::ContentBox,
                "border-box" => self.box_sizing = BoxSizing::BorderBox,
                _ => {}
            },
            name if name.starts_with("margin") => {
                apply_edges(
                    &mut self.margin,
                    name,
                    value,
                    "margin",
                    (self.font_size, root_font, viewport),
                    (true, true),
                );
            }
            name if name.starts_with("padding") => {
                apply_edges(
                    &mut self.padding,
                    name,
                    value,
                    "padding",
                    (self.font_size, root_font, viewport),
                    (false, false),
                );
            }
            "border-width" => apply_edges(
                &mut self.border_width,
                "border-width",
                value,
                "border-width",
                (self.font_size, root_font, viewport),
                (false, false),
            ),
            "border-top-width"
            | "border-right-width"
            | "border-bottom-width"
            | "border-left-width" => {
                if let Some(side) = declaration.name.strip_suffix("-width") {
                    apply_edges(
                        &mut self.border_width,
                        side,
                        value,
                        "border",
                        (self.font_size, root_font, viewport),
                        (false, false),
                    );
                }
            }
            "border-style" => match value {
                "none" => self.border_style = BorderStyle::None,
                "solid" => self.border_style = BorderStyle::Solid,
                "dashed" => self.border_style = BorderStyle::Dashed,
                "dotted" => self.border_style = BorderStyle::Dotted,
                _ => {}
            },
            "border-color" => {
                if let Some(color) = Color::parse(value) {
                    self.border_color = color;
                    self.border_color_explicit = true;
                } else if value == "currentcolor" {
                    self.border_color_explicit = false;
                }
            }
            "border" => {
                let Some(parts) = crate::values::components(value) else {
                    return;
                };
                let mut parsed = self.clone();
                let mut width_seen = false;
                let mut color_seen = false;
                let mut style_seen = false;
                for part in parts {
                    if let Some(width) = unit(part, false, false) {
                        if width_seen {
                            return;
                        }
                        width_seen = true;
                        parsed.border_width = Edges::shorthand(&[width]).unwrap();
                    } else if let Some(color) = Color::parse(part) {
                        if color_seen {
                            return;
                        }
                        color_seen = true;
                        parsed.border_color = color;
                        parsed.border_color_explicit = true;
                    } else if part == "currentcolor" {
                        if color_seen {
                            return;
                        }
                        color_seen = true;
                        parsed.border_color_explicit = false;
                    } else {
                        if style_seen {
                            return;
                        }
                        style_seen = true;
                        match part {
                            "solid" => parsed.border_style = BorderStyle::Solid,
                            "dashed" => parsed.border_style = BorderStyle::Dashed,
                            "dotted" => parsed.border_style = BorderStyle::Dotted,
                            "none" => parsed.border_style = BorderStyle::None,
                            _ => return,
                        }
                    }
                }
                self.border_width = parsed.border_width;
                self.border_color = parsed.border_color;
                self.border_color_explicit = parsed.border_color_explicit;
                self.border_style = parsed.border_style;
            }
            "border-radius" => apply_edges(
                &mut self.border_radius,
                "border-radius",
                value,
                "border-radius",
                (self.font_size, root_font, viewport),
                (false, false),
            ),
            "line-height" => {
                if value == "normal" {
                    self.line_height = self.font_size * 1.2;
                    self.line_height_factor = Some(1.2);
                } else if let Ok(multiplier) = value.parse::<f32>() {
                    if multiplier.is_finite() && (0.1..=10.0).contains(&multiplier) {
                        self.line_height = self.font_size * multiplier;
                        self.line_height_factor = Some(multiplier);
                    }
                } else if let Some(line) =
                    unit(value, false, false).and_then(|line| line.resolve(self.font_size))
                    && (1.0..=1_000_000.0).contains(&line)
                {
                    self.line_height = line;
                    self.line_height_factor = None;
                }
            }
            "text-align" => match value {
                "left" => self.text_align = TextAlign::Left,
                "start" => self.text_align = TextAlign::Start,
                "center" => self.text_align = TextAlign::Center,
                "right" => self.text_align = TextAlign::Right,
                "end" => self.text_align = TextAlign::End,
                _ => {}
            },
            "white-space" => match value {
                "normal" => self.white_space = WhiteSpace::Normal,
                "nowrap" => self.white_space = WhiteSpace::NoWrap,
                "pre" => self.white_space = WhiteSpace::Pre,
                "pre-wrap" => self.white_space = WhiteSpace::PreWrap,
                "pre-line" => self.white_space = WhiteSpace::PreLine,
                _ => {}
            },
            "overflow" => match value {
                "hidden" => self.overflow_hidden = true,
                "visible" => self.overflow_hidden = false,
                _ => {}
            },
            _ => {}
        }
    }
}

fn user_agent_style(tag: &str, style: &mut ComputedStyle) {
    let inherited_size = style.font_size;
    if matches!(
        tag,
        "html"
            | "body"
            | "main"
            | "header"
            | "footer"
            | "section"
            | "article"
            | "nav"
            | "div"
            | "p"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "ul"
            | "ol"
            | "li"
            | "pre"
            | "blockquote"
            | "form"
            | "hr"
    ) {
        style.display = Display::Block;
    }
    if matches!(tag, "head" | "style" | "script" | "meta" | "link" | "title") {
        style.display = Display::None;
    }
    match tag {
        "body" => {
            style.margin = Edges {
                top: Length::Px(8.0),
                right: Length::Px(8.0),
                bottom: Length::Px(8.0),
                left: Length::Px(8.0),
            }
        }
        "p" => {
            style.margin.top = Length::Px(16.0);
            style.margin.bottom = Length::Px(16.0);
        }
        "h1" => {
            style.font_size = 32.0;
            style.bold = true;
            style.margin.top = Length::Px(21.0);
            style.margin.bottom = Length::Px(21.0);
        }
        "h2" => {
            style.font_size = 24.0;
            style.bold = true;
            style.margin.top = Length::Px(19.0);
            style.margin.bottom = Length::Px(19.0);
        }
        "h3" => {
            style.font_size = 19.0;
            style.bold = true;
            style.margin.top = Length::Px(16.0);
            style.margin.bottom = Length::Px(16.0);
        }
        "b" | "strong" => style.bold = true,
        "small" => style.font_size *= 0.8,
        "pre" => style.white_space = WhiteSpace::Pre,
        "ul" => {
            style.padding.left = Length::Px(40.0);
            style.list_style_type = ListStyleType::Disc;
        }
        "ol" => {
            style.padding.left = Length::Px(40.0);
            style.list_style_type = ListStyleType::Decimal;
        }
        "img" => style.display = Display::InlineBlock,
        _ => {}
    }
    if style.font_size != inherited_size
        && let Some(factor) = style.line_height_factor
    {
        style.line_height = style.font_size * factor;
    }
}

fn compute_node(
    document: &Document,
    sheet: &Stylesheet,
    styles: &mut [ComputedStyle],
    id: NodeId,
    inherited: &ComputedStyle,
    root_font: f32,
    work: &mut CascadeWork,
) {
    let viewport = work.viewport;
    let mut style = ComputedStyle::inherit(inherited);
    work.charge(1);
    if let Some(element) = document.element(id) {
        user_agent_style(&element.tag, &mut style);
        if let Some(direction) = element.attribute("dir") {
            if direction.eq_ignore_ascii_case("ltr") {
                style.direction = Direction::Ltr;
            } else if direction.eq_ignore_ascii_case("rtl") {
                style.direction = Direction::Rtl;
            }
        }
        if element.tag == "bdi" {
            style.unicode_bidi = UnicodeBidi::Isolate;
        }
        // Author declarations are ordered by importance, style attributes,
        // selector specificity, then their exact order in the source.
        let mut matched = Vec::new();
        for active in 0..work.active_rules.len() {
            let order = work.active_rules[active];
            let rule = &sheet.rules[order];
            if !work.charge(1) {
                break;
            }
            let mut specificity: Option<(usize, usize, usize)> = None;
            for selector in &rule.selectors {
                match selector.matches_bounded(document, id, &mut work.remaining) {
                    Some(true) => {
                        specificity = Some(
                            specificity
                                .map_or(selector.specificity, |old| old.max(selector.specificity)),
                        )
                    }
                    Some(false) => {}
                    None => {
                        work.truncated = true;
                        break;
                    }
                }
            }
            if let Some(specificity) = specificity {
                for (declaration_order, declaration) in rule.declarations.iter().enumerate() {
                    if !work.charge(1 + declaration.name.len() + declaration.value.len()) {
                        break;
                    }
                    matched.push((
                        declaration.important,
                        0u8,
                        specificity,
                        order,
                        declaration_order,
                        declaration,
                    ));
                }
            }
        }
        let inline = element.attribute("style").unwrap_or("");
        let (declarations, truncated) = if inline.is_empty() {
            (Vec::new(), false)
        } else if work.charge(inline.len()) {
            css::parse_declarations_with_status(inline)
        } else {
            (Vec::new(), true)
        };
        work.truncated |= truncated;
        for (declaration_order, declaration) in declarations.iter().enumerate() {
            matched.push((
                declaration.important,
                1u8,
                (0, 0, 0),
                sheet.rules.len(),
                declaration_order,
                declaration,
            ));
        }
        matched.sort_by_key(
            |(important, attribute, specificity, rule_order, declaration_order, _)| {
                (
                    *important,
                    *attribute,
                    *specificity,
                    *rule_order,
                    *declaration_order,
                )
            },
        );
        // Relative values and currentcolor use their cascaded environment,
        // independently of their declarations' order.
        for (_, _, _, _, _, declaration) in &matched {
            if declaration.name == "font-size" || declaration.name == "color" {
                style.apply(declaration, inherited.font_size, root_font, viewport);
            }
        }
        let property_root_font = if element.tag == "html" {
            style.font_size
        } else {
            root_font
        };
        for (_, _, _, _, _, declaration) in matched {
            if declaration.name != "font-size" && declaration.name != "color" {
                style.apply(
                    declaration,
                    inherited.font_size,
                    property_root_font,
                    viewport,
                );
            }
        }
    }
    if !style.border_color_explicit {
        style.border_color = style.color;
    }
    if style.position == Position::Absolute && style.display != Display::None {
        style.display = match style.display {
            Display::InlineFlex => Display::Flex,
            Display::InlineGrid => Display::Grid,
            Display::Flex | Display::Grid => style.display,
            _ => Display::Block,
        };
    }
    styles[id] = style.clone();
    let root_font = if document
        .element(id)
        .is_some_and(|element| element.tag == "html")
    {
        style.font_size
    } else {
        root_font
    };
    if matches!(document.nodes[id].kind, NodeKind::TemplateContent)
        || document.element(id).is_some_and(|e| e.tag == "template")
    {
        return;
    }
    for &child in &document.nodes[id].children {
        compute_node(document, sheet, styles, child, &style, root_font, work);
    }
}

pub fn compute(document: &Document, sheet: &Stylesheet) -> Vec<ComputedStyle> {
    compute_with_viewport(document, sheet, crate::values::Viewport::default())
}

/// Aggregate selector, ancestor, declaration-byte and inline parsing work.
pub const MAX_CASCADE_WORK: usize = 8_000_000;

struct CascadeWork {
    remaining: usize,
    truncated: bool,
    viewport: crate::values::Viewport,
    active_rules: Vec<usize>,
}
impl CascadeWork {
    fn charge(&mut self, amount: usize) -> bool {
        if amount > self.remaining {
            self.remaining = 0;
            self.truncated = true;
            false
        } else {
            self.remaining -= amount;
            true
        }
    }
}

pub struct ComputedStyles {
    pub styles: Vec<ComputedStyle>,
    pub truncated: bool,
}

pub fn compute_with_viewport(
    document: &Document,
    sheet: &Stylesheet,
    viewport: crate::values::Viewport,
) -> Vec<ComputedStyle> {
    compute_with_status(document, sheet, viewport).styles
}

/// Exhaustion leaves the deterministically accepted author prefix, with inherited
/// and user-agent defaults for later nodes, and reports omitted work explicitly.
pub fn compute_with_status(
    document: &Document,
    sheet: &Stylesheet,
    viewport: crate::values::Viewport,
) -> ComputedStyles {
    let mut styles = vec![ComputedStyle::default(); document.nodes.len()];
    let mut work = CascadeWork {
        remaining: MAX_CASCADE_WORK,
        truncated: false,
        viewport,
        active_rules: Vec::new(),
    };
    // Every node sees the same viewport, so conditions are evaluated once,
    // retaining their original rule indices for the existing cascade.
    for (index, rule) in sheet.rules.iter().enumerate() {
        let cost = 1 + rule
            .media
            .iter()
            .map(|list| {
                1 + list
                    .alternatives
                    .iter()
                    .map(|query| 1 + query.constraints.len())
                    .sum::<usize>()
            })
            .sum::<usize>();
        if !work.charge(cost) {
            break;
        }
        if rule.is_active(viewport.width) {
            work.active_rules.push(index);
        }
    }
    compute_node(
        document,
        sheet,
        &mut styles,
        0,
        &ComputedStyle::default(),
        16.0,
        &mut work,
    );
    ComputedStyles {
        styles,
        truncated: work.truncated || sheet.truncated,
    }
}

pub fn is_visible(document: &Document, styles: &[ComputedStyle], node: NodeId) -> bool {
    if matches!(document.nodes[node].kind, NodeKind::TemplateContent)
        || matches!(&document.nodes[node].kind, NodeKind::Element(element) if element.namespace == crate::dom::Namespace::Html && element.tag == "template")
    {
        return false;
    }
    if styles[node].display == Display::None {
        return false;
    }
    !matches!(&document.nodes[node].kind, NodeKind::Element(element) if element.tag == "script")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html;

    fn by_id(document: &Document, value: &str) -> NodeId {
        document
            .nodes
            .iter()
            .position(|node| matches!(&node.kind, NodeKind::Element(element) if element.attribute("id") == Some(value)))
            .unwrap()
    }

    #[test]
    fn positioned_styles_are_bounded_and_not_inherited() {
        let document = html::parse("<div id=p style='position:relative;inset:1em 10% -3px auto;z-index:-32768;direction:rtl;unicode-bidi:isolate;overflow-wrap:anywhere'><span id=c>text</span></div><span id=a style='position:absolute;z-index:32767;left:-2rem;top:50%'>overlay</span><div id=bad style='position:fixed;left:NaNpx;z-index:32768'></div>").unwrap();
        let styles = compute(&document, &Stylesheet::default());
        let parent = &styles[by_id(&document, "p")];
        let child = &styles[by_id(&document, "c")];
        let absolute = &styles[by_id(&document, "a")];
        let bad = &styles[by_id(&document, "bad")];
        assert_eq!(parent.position, Position::Relative);
        assert_eq!(parent.insets.top, Length::Px(16.0));
        assert_eq!(parent.insets.right, Length::Percent(10.0));
        assert_eq!(parent.insets.bottom, Length::Px(-3.0));
        assert_eq!(parent.insets.left, Length::Auto);
        assert_eq!(parent.z_index, Some(-32_768));
        assert_eq!(child.position, Position::Static);
        assert_eq!(child.insets.top, Length::Auto);
        assert_eq!(child.z_index, None);
        assert_eq!(child.direction, Direction::Rtl);
        assert_eq!(child.unicode_bidi, UnicodeBidi::Normal);
        assert_eq!(child.overflow_wrap, OverflowWrap::Anywhere);
        assert_eq!(absolute.display, Display::Block);
        assert_eq!(absolute.insets.left, Length::Px(-32.0));
        assert_eq!(absolute.insets.top, Length::Percent(50.0));
        assert_eq!(absolute.z_index, Some(32_767));
        assert_eq!(bad.position, Position::Static);
        assert_eq!(bad.insets.left, Length::Auto);
        assert_eq!(bad.z_index, None);
    }

    #[test]
    fn direction_and_logical_alignment_follow_inheritance_and_cascade() {
        let document = html::parse("<div dir=rtl><span id=logical style='text-align:end'>text</span><span id=override dir=ltr style='direction:rtl;unicode-bidi:embed'>text</span><bdi id=isolate>Latin</bdi></div>").unwrap();
        let styles = compute(&document, &Stylesheet::default());
        let logical = &styles[by_id(&document, "logical")];
        assert_eq!(logical.direction, Direction::Rtl);
        assert_eq!(logical.text_align, TextAlign::End);
        assert_eq!(
            logical.text_align.physical(logical.direction),
            TextAlign::Left
        );
        assert_eq!(TextAlign::Start.physical(Direction::Rtl), TextAlign::Right);
        assert_eq!(TextAlign::End.physical(Direction::Ltr), TextAlign::Right);
        let overridden = &styles[by_id(&document, "override")];
        assert_eq!(overridden.direction, Direction::Rtl);
        assert_eq!(overridden.unicode_bidi, UnicodeBidi::Embed);
        assert_eq!(
            styles[by_id(&document, "isolate")].unicode_bidi,
            UnicodeBidi::Isolate
        );
    }

    #[test]
    fn inherited_unitless_line_height_tracks_font_size_but_lengths_do_not() {
        let document = html::parse("<div style='font-size:20px;line-height:1.5'><span id=factor style='font-size:40px'>text</span></div><div style='line-height:30px'><span id=length style='font-size:40px'>text</span><h1 id=heading>text</h1></div><span id=order style='line-height:2;font-size:25px'>text</span>").unwrap();
        let styles = compute(&document, &Stylesheet::default());
        let factor = &styles[by_id(&document, "factor")];
        assert_eq!(factor.line_height, 60.0);
        assert_eq!(factor.line_height_factor, Some(1.5));
        assert_eq!(styles[by_id(&document, "length")].line_height, 30.0);
        assert_eq!(styles[by_id(&document, "heading")].line_height, 30.0);
        assert_eq!(styles[by_id(&document, "order")].line_height, 50.0);
    }

    #[test]
    fn font_relative_values_use_cascaded_size_independent_of_declaration_order() {
        let document = html::parse("<style>.sheet{line-height:150%;padding:1em;width:10em;font-size:20px}#winning{font-size:30px!important}</style><p id=sheet class=sheet>text</p><p id=early style='font-size:20px;line-height:150%;padding:1em;width:10em'>text</p><p id=late style='line-height:150%;padding:1em;width:10em;font-size:20px'>text</p><p id=winning class=sheet style='font-size:10px'>text</p>").unwrap();
        let styles = compute(&document, &css::parse(&document.stylesheets()));
        for id in ["sheet", "early", "late", "winning"] {
            let style = &styles[by_id(&document, id)];
            let size = if id == "winning" { 30.0 } else { 20.0 };
            assert_eq!(style.font_size, size);
            assert_eq!(style.line_height, size * 1.5);
            assert_eq!(style.line_height_factor, None);
            assert_eq!(style.padding.top, Length::Px(size));
            assert_eq!(style.padding.left, Length::Px(size));
            assert_eq!(style.width, Some(Length::Px(size * 10.0)));
        }
        let root = html::parse("<html id=root style='padding:1rem;width:10rem;line-height:1.5rem;font-size:2rem'><p>text</p></html>").unwrap();
        let styles = compute(&root, &Stylesheet::default());
        let style = &styles[by_id(&root, "root")];
        assert_eq!(style.font_size, 32.0);
        assert_eq!(style.line_height, 48.0);
        assert_eq!(style.padding.top, Length::Px(32.0));
        assert_eq!(style.width, Some(Length::Px(320.0)));
    }

    #[test]
    fn cascade_and_inheritance() {
        let document = html::parse(
            "<style>p { color: red } #x { color: blue } p.hot { color: green }</style>\
             <p id='x' class='hot' style='font-size: 20px'>Hello</p>",
        )
        .unwrap();
        let styles = compute(&document, &css::parse(&document.stylesheets()));
        let paragraph = document
            .nodes
            .iter()
            .position(|node| matches!(&node.kind, NodeKind::Element(element) if element.tag == "p"))
            .unwrap();
        assert_eq!(styles[paragraph].color, Color(0, 0, 255, 255));
        assert_eq!(styles[paragraph].font_size, 20.0);
        assert_eq!(
            styles[document.nodes[paragraph].children[0]].color,
            Color(0, 0, 255, 255)
        );
    }

    #[test]
    fn important_specificity_and_declaration_order() {
        let document = html::parse("<p id='x' class='hot' style='color: blue; color: red'>one</p><p id='y' style='color: blue !important'>two</p>").unwrap();
        let sheet = css::parse(
            "p { color: green !important; color: navy } #x { color: purple !important } .hot { color: red !important }",
        );
        let styles = compute(&document, &sheet);
        let x = document.nodes.iter().position(|node| matches!(&node.kind, NodeKind::Element(element) if element.attribute("id") == Some("x"))).unwrap();
        let y = document.nodes.iter().position(|node| matches!(&node.kind, NodeKind::Element(element) if element.attribute("id") == Some("y"))).unwrap();
        assert_eq!(styles[x].color, Color(128, 0, 128, 255));
        assert_eq!(styles[y].color, Color(0, 0, 255, 255));
    }
}
