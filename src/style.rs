use crate::css::{self, Declaration, Stylesheet};
use crate::dom::{Document, NodeId, NodeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    Block,
    Inline,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8);

impl Color {
    pub const BLACK: Self = Self(0, 0, 0);
    pub const WHITE: Self = Self(255, 255, 255);

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
                    Some(Self(r, g, b))
                }
                6 => Some(Self(
                    u8::from_str_radix(&hex[0..2], 16).ok()?,
                    u8::from_str_radix(&hex[2..4], 16).ok()?,
                    u8::from_str_radix(&hex[4..6], 16).ok()?,
                )),
                _ => None,
            };
        }
        match value {
            "black" => Some(Self::BLACK),
            "white" => Some(Self::WHITE),
            "red" => Some(Self(255, 0, 0)),
            "green" => Some(Self(0, 128, 0)),
            "blue" => Some(Self(0, 0, 255)),
            "navy" => Some(Self(0, 0, 128)),
            "teal" => Some(Self(0, 128, 128)),
            "gray" | "grey" => Some(Self(128, 128, 128)),
            "silver" => Some(Self(192, 192, 192)),
            "yellow" => Some(Self(255, 255, 0)),
            "orange" => Some(Self(255, 165, 0)),
            "purple" => Some(Self(128, 0, 128)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Edges {
    fn shorthand(values: &[f32]) -> Option<Self> {
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
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub margin: Edges,
    pub padding: Edges,
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
            margin: Edges::default(),
            padding: Edges::default(),
        }
    }
}

fn pixels(value: &str) -> Option<f32> {
    let number = value.strip_suffix("px").unwrap_or(value).trim();
    let parsed: f32 = number.parse().ok()?;
    (parsed.is_finite() && (0.0..=16_384.0).contains(&parsed)).then_some(parsed)
}

fn apply_edges(edges: &mut Edges, property: &str, value: &str, prefix: &str) {
    if property == prefix {
        let numbers: Option<Vec<_>> = value.split_ascii_whitespace().map(pixels).collect();
        if let Some(parsed) = numbers.and_then(|numbers| Edges::shorthand(&numbers)) {
            *edges = parsed;
        }
        return;
    }
    let Some(value) = pixels(value) else {
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
            ..Self::default()
        }
    }

    fn apply(&mut self, declaration: &Declaration) {
        let value = declaration.value.as_str();
        match declaration.name.as_str() {
            "display" => match value {
                "block" => self.display = Display::Block,
                "inline" => self.display = Display::Inline,
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
                if let Some(size) = pixels(value).filter(|size| *size >= 1.0) {
                    self.font_size = size;
                }
            }
            "font-weight" => match value {
                "bold" | "700" | "800" | "900" => self.bold = true,
                "normal" | "400" => self.bold = false,
                _ => {}
            },
            "width" => self.width = pixels(value),
            "height" => self.height = pixels(value),
            name if name.starts_with("margin") => {
                apply_edges(&mut self.margin, name, value, "margin");
            }
            name if name.starts_with("padding") => {
                apply_edges(&mut self.padding, name, value, "padding");
            }
            _ => {}
        }
    }
}

fn user_agent_style(tag: &str, style: &mut ComputedStyle) {
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
                top: 8.0,
                right: 8.0,
                bottom: 8.0,
                left: 8.0,
            }
        }
        "p" => {
            style.margin.top = 16.0;
            style.margin.bottom = 16.0;
        }
        "h1" => {
            style.font_size = 32.0;
            style.bold = true;
            style.margin.top = 21.0;
            style.margin.bottom = 21.0;
        }
        "h2" => {
            style.font_size = 24.0;
            style.bold = true;
            style.margin.top = 19.0;
            style.margin.bottom = 19.0;
        }
        "h3" => {
            style.font_size = 19.0;
            style.bold = true;
            style.margin.top = 16.0;
            style.margin.bottom = 16.0;
        }
        "b" | "strong" => style.bold = true,
        "small" => style.font_size *= 0.8,
        _ => {}
    }
}

fn compute_node(
    document: &Document,
    sheet: &Stylesheet,
    styles: &mut [ComputedStyle],
    id: NodeId,
    inherited: &ComputedStyle,
) {
    let mut style = ComputedStyle::inherit(inherited);
    if let Some(element) = document.element(id) {
        user_agent_style(&element.tag, &mut style);
        let mut matched = Vec::new();
        for (order, rule) in sheet.rules.iter().enumerate() {
            for selector in &rule.selectors {
                if selector.matches(document, id) {
                    matched.push((selector.specificity, order, &rule.declarations));
                }
            }
        }
        matched.sort_by_key(|(specificity, order, _)| (*specificity, *order));
        for (_, _, declarations) in matched {
            for declaration in declarations {
                style.apply(declaration);
            }
        }
        if let Some(inline) = element.attribute("style") {
            for declaration in css::parse_declarations(inline) {
                style.apply(&declaration);
            }
        }
    }
    styles[id] = style.clone();
    for &child in &document.nodes[id].children {
        compute_node(document, sheet, styles, child, &style);
    }
}

pub fn compute(document: &Document, sheet: &Stylesheet) -> Vec<ComputedStyle> {
    let mut styles = vec![ComputedStyle::default(); document.nodes.len()];
    compute_node(document, sheet, &mut styles, 0, &ComputedStyle::default());
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
        assert_eq!(styles[paragraph].color, Color(0, 0, 255));
        assert_eq!(styles[paragraph].font_size, 20.0);
        assert_eq!(
            styles[document.nodes[paragraph].children[0]].color,
            Color(0, 0, 255)
        );
    }
}
