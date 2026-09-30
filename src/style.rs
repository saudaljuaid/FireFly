use crate::css::{self, Declaration, Stylesheet};
use crate::dom::{Document, NodeId, NodeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    Block,
    Inline,
    InlineBlock,
    None,
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
    Auto,
}

impl Default for Length {
    fn default() -> Self {
        Self::Px(0.0)
    }
}

impl Length {
    pub fn resolve(self, base: f32) -> Option<f32> {
        match self {
            Self::Px(value) => Some(value),
            Self::Percent(value) => Some(base * value / 100.0),
            Self::Auto => None,
        }
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
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhiteSpace {
    Normal,
    NoWrap,
    Pre,
    PreWrap,
    PreLine,
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
    pub color: Color,
    pub background: Option<Color>,
    pub font_size: f32,
    pub bold: bool,
    pub width: Option<Length>,
    pub height: Option<Length>,
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
    pub text_align: TextAlign,
    pub white_space: WhiteSpace,
    pub overflow_hidden: bool,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display: Display::Inline,
            color: Color::BLACK,
            background: None,
            font_size: 16.0,
            bold: false,
            width: None,
            height: None,
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
            text_align: TextAlign::Left,
            white_space: WhiteSpace::Normal,
            overflow_hidden: false,
        }
    }
}

fn length(
    value: &str,
    em: f32,
    rem: f32,
    allow_auto: bool,
    allow_negative: bool,
) -> Option<Length> {
    let value = value.trim().to_ascii_lowercase();
    if allow_auto && value == "auto" {
        return Some(Length::Auto);
    }
    let (number, unit) = if let Some(number) = value.strip_suffix("rem") {
        (number, "rem")
    } else if let Some(number) = value.strip_suffix("em") {
        (number, "em")
    } else if let Some(number) = value.strip_suffix("px") {
        (number, "px")
    } else if let Some(number) = value.strip_suffix('%') {
        (number, "%")
    } else {
        (value.as_str(), "")
    };
    let parsed: f32 = number.trim().parse().ok()?;
    if !parsed.is_finite() || parsed.abs() > 16_384.0 || (!allow_negative && parsed < 0.0) {
        return None;
    }
    match unit {
        "rem" => Some(Length::Px(parsed * rem)),
        "em" => Some(Length::Px(parsed * em)),
        "%" => Some(Length::Percent(parsed)),
        "px" => Some(Length::Px(parsed)),
        "" if parsed == 0.0 => Some(Length::Px(0.0)),
        _ => None,
    }
}

fn apply_edges(
    edges: &mut Edges,
    property: &str,
    value: &str,
    prefix: &str,
    units: (f32, f32),
    allowances: (bool, bool),
) {
    let (em, rem) = units;
    let (allow_auto, allow_negative) = allowances;
    if property == prefix {
        let numbers: Option<Vec<_>> = value
            .split_ascii_whitespace()
            .map(|part| length(part, em, rem, allow_auto, allow_negative))
            .collect();
        if let Some(parsed) = numbers.and_then(|numbers| Edges::shorthand(&numbers)) {
            *edges = parsed;
        }
        return;
    }
    let Some(value) = length(value, em, rem, allow_auto, allow_negative) else {
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
            text_align: parent.text_align,
            white_space: parent.white_space,
            ..Self::default()
        }
    }

    fn apply(&mut self, declaration: &Declaration, parent_font: f32, root_font: f32) {
        let lower = declaration.value.to_ascii_lowercase();
        let value = lower.as_str();
        let font_size = self.font_size;
        let unit = |part: &str, auto: bool, negative: bool| {
            length(part, font_size, root_font, auto, negative)
        };
        match declaration.name.as_str() {
            "display" => match value {
                "block" => self.display = Display::Block,
                "inline" => self.display = Display::Inline,
                "inline-block" => self.display = Display::InlineBlock,
                "none" => self.display = Display::None,
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
                }
            }
            "font-size" => {
                if let Some(size) = length(value, parent_font, root_font, false, false)
                    .and_then(|length| length.resolve(parent_font))
                    .filter(|size| (1.0..=1024.0).contains(size))
                {
                    self.font_size = size;
                    self.line_height = size * 1.2;
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
                } else if let Some(size) = unit(value, false, false) {
                    self.width = Some(size);
                }
            }
            "height" => {
                if value == "auto" {
                    self.height = None;
                } else if let Some(size) = unit(value, false, false) {
                    self.height = Some(size);
                }
            }
            "min-width" => {
                if let Some(size) = unit(value, false, false) {
                    self.min_width = Some(size);
                }
            }
            "max-width" => {
                if value == "none" {
                    self.max_width = None;
                } else if let Some(size) = unit(value, false, false) {
                    self.max_width = Some(size);
                }
            }
            "min-height" => {
                if let Some(size) = unit(value, false, false) {
                    self.min_height = Some(size);
                }
            }
            "max-height" => {
                if value == "none" {
                    self.max_height = None;
                } else if let Some(size) = unit(value, false, false) {
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
                    (self.font_size, root_font),
                    (true, true),
                );
            }
            name if name.starts_with("padding") => {
                apply_edges(
                    &mut self.padding,
                    name,
                    value,
                    "padding",
                    (self.font_size, root_font),
                    (false, false),
                );
            }
            "border-width" => apply_edges(
                &mut self.border_width,
                "border-width",
                value,
                "border-width",
                (self.font_size, root_font),
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
                        (self.font_size, root_font),
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
                for part in value.split_ascii_whitespace() {
                    if let Some(width) = unit(part, false, false) {
                        self.border_width = Edges::shorthand(&[width]).unwrap();
                    } else if let Some(color) = Color::parse(part) {
                        self.border_color = color;
                        self.border_color_explicit = true;
                    } else if part == "currentcolor" {
                        self.border_color_explicit = false;
                    } else {
                        match part {
                            "solid" => self.border_style = BorderStyle::Solid,
                            "dashed" => self.border_style = BorderStyle::Dashed,
                            "dotted" => self.border_style = BorderStyle::Dotted,
                            "none" => self.border_style = BorderStyle::None,
                            _ => {}
                        }
                    }
                }
            }
            "border-radius" => apply_edges(
                &mut self.border_radius,
                "border-radius",
                value,
                "border-radius",
                (self.font_size, root_font),
                (false, false),
            ),
            "line-height" => {
                if value == "normal" {
                    self.line_height = self.font_size * 1.2;
                } else if let Ok(multiplier) = value.parse::<f32>() {
                    if multiplier.is_finite() && (0.1..=10.0).contains(&multiplier) {
                        self.line_height = self.font_size * multiplier;
                    }
                } else if let Some(line) =
                    unit(value, false, false).and_then(|line| line.resolve(self.font_size))
                    && line >= 1.0
                {
                    self.line_height = line;
                }
            }
            "text-align" => match value {
                "left" | "start" => self.text_align = TextAlign::Left,
                "center" => self.text_align = TextAlign::Center,
                "right" | "end" => self.text_align = TextAlign::Right,
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
        "img" => style.display = Display::InlineBlock,
        _ => {}
    }
    if style.font_size != inherited_size {
        style.line_height = style.font_size * 1.2;
    }
}

fn compute_node(
    document: &Document,
    sheet: &Stylesheet,
    styles: &mut [ComputedStyle],
    id: NodeId,
    inherited: &ComputedStyle,
    root_font: f32,
) {
    let mut style = ComputedStyle::inherit(inherited);
    if let Some(element) = document.element(id) {
        user_agent_style(&element.tag, &mut style);
        // Author declarations are ordered by importance, style attributes,
        // selector specificity, then their exact order in the source.
        let mut matched = Vec::new();
        for (order, rule) in sheet.rules.iter().enumerate() {
            for selector in &rule.selectors {
                if selector.matches(document, id) {
                    for (declaration_order, declaration) in rule.declarations.iter().enumerate() {
                        matched.push((
                            declaration.important,
                            0u8,
                            selector.specificity,
                            order,
                            declaration_order,
                            declaration,
                        ));
                    }
                }
            }
        }
        if let Some(inline) = element.attribute("style") {
            let declarations = css::parse_declarations(inline);
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
            for (_, _, _, _, _, declaration) in matched {
                style.apply(declaration, inherited.font_size, root_font);
            }
        } else {
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
            for (_, _, _, _, _, declaration) in matched {
                style.apply(declaration, inherited.font_size, root_font);
            }
        }
    }
    if !style.border_color_explicit {
        style.border_color = style.color;
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
    for &child in &document.nodes[id].children {
        compute_node(document, sheet, styles, child, &style, root_font);
    }
}

pub fn compute(document: &Document, sheet: &Stylesheet) -> Vec<ComputedStyle> {
    let mut styles = vec![ComputedStyle::default(); document.nodes.len()];
    compute_node(
        document,
        sheet,
        &mut styles,
        0,
        &ComputedStyle::default(),
        16.0,
    );
    styles
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
