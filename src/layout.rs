use crate::dom::{Document, NodeId, NodeKind};
use crate::style::{Color, ComputedStyle, Display, is_visible};

#[derive(Debug, Clone, PartialEq)]
pub enum Primitive {
    Rectangle {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
    },
    Text {
        x: f32,
        baseline: f32,
        content: String,
        size: f32,
        color: Color,
        bold: bool,
    },
}

#[derive(Debug, Clone)]
pub struct Scene {
    pub width: f32,
    pub height: f32,
    pub primitives: Vec<Primitive>,
}

struct Builder<'a> {
    document: &'a Document,
    styles: &'a [ComputedStyle],
    primitives: Vec<Primitive>,
}

struct Line {
    start_x: f32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    has_text: bool,
}

impl Line {
    fn new(x: f32, y: f32, width: f32) -> Self {
        Self {
            start_x: x,
            x,
            y,
            width,
            height: 0.0,
            has_text: false,
        }
    }

    fn break_line(&mut self) {
        self.y += self.height.max(19.0);
        self.x = self.start_x;
        self.height = 0.0;
        self.has_text = false;
    }

    fn finish(&self) -> f32 {
        self.y + self.height
    }
}

fn glyph_width(character: char, size: f32) -> f32 {
    let ratio = match character {
        'i' | 'l' | 'I' | '!' | '.' | ',' | ':' | ';' | '\'' => 0.28,
        'm' | 'w' | 'M' | 'W' | '@' => 0.82,
        ' ' => 0.32,
        _ if character.is_ascii() => 0.55,
        _ => 0.65,
    };
    ratio * size
}

fn text_width(text: &str, size: f32) -> f32 {
    text.chars()
        .map(|character| glyph_width(character, size))
        .sum()
}

impl Builder<'_> {
    fn text(&mut self, content: &str, style: &ComputedStyle, line: &mut Line) {
        for word in content.split_whitespace() {
            let gap = if line.has_text {
                glyph_width(' ', style.font_size)
            } else {
                0.0
            };
            let width = text_width(word, style.font_size);
            if line.has_text && line.x + gap + width > line.start_x + line.width {
                line.break_line();
            }
            if width > line.width && line.width > 0.0 {
                self.long_word(word, style, line);
                continue;
            }
            if line.has_text {
                line.x += gap;
            }
            self.primitives.push(Primitive::Text {
                x: line.x,
                baseline: line.y + style.font_size,
                content: word.into(),
                size: style.font_size,
                color: style.color,
                bold: style.bold,
            });
            line.x += width;
            line.height = line.height.max(style.font_size * 1.25);
            line.has_text = true;
        }
    }

    fn long_word(&mut self, word: &str, style: &ComputedStyle, line: &mut Line) {
        let mut chunk = String::new();
        let mut width = 0.0;
        for character in word.chars() {
            let advance = glyph_width(character, style.font_size);
            if !chunk.is_empty() && line.x + width + advance > line.start_x + line.width {
                self.primitives.push(Primitive::Text {
                    x: line.x,
                    baseline: line.y + style.font_size,
                    content: std::mem::take(&mut chunk),
                    size: style.font_size,
                    color: style.color,
                    bold: style.bold,
                });
                line.break_line();
                width = 0.0;
            }
            chunk.push(character);
            width += advance;
        }
        if !chunk.is_empty() {
            self.primitives.push(Primitive::Text {
                x: line.x,
                baseline: line.y + style.font_size,
                content: chunk,
                size: style.font_size,
                color: style.color,
                bold: style.bold,
            });
            line.x += width;
            line.height = line.height.max(style.font_size * 1.25);
            line.has_text = true;
        }
    }

    fn inline(&mut self, id: NodeId, line: &mut Line) {
        if !is_visible(self.document, self.styles, id) {
            return;
        }
        match &self.document.nodes[id].kind {
            NodeKind::Text(text) => self.text(text, &self.styles[id], line),
            NodeKind::Element(element) if element.tag == "br" => line.break_line(),
            _ => {
                for &child in &self.document.nodes[id].children {
                    self.inline(child, line);
                }
            }
        }
    }

    fn children(&mut self, id: NodeId, x: f32, y: f32, width: f32) -> f32 {
        let mut line = Line::new(x, y, width);
        for &child in &self.document.nodes[id].children {
            if !is_visible(self.document, self.styles, child) {
                continue;
            }
            if self.styles[child].display == Display::Block {
                let block_y = line.finish();
                let height = self.block(child, x, block_y, width);
                line = Line::new(x, block_y + height, width);
            } else {
                self.inline(child, &mut line);
            }
        }
        line.finish() - y
    }

    fn block(&mut self, id: NodeId, x: f32, y: f32, available_width: f32) -> f32 {
        let style = &self.styles[id];
        let outer_x = x + style.margin.left;
        let outer_y = y + style.margin.top;
        let max_width = (available_width - style.margin.left - style.margin.right).max(0.0);
        let content_width = style
            .width
            .unwrap_or(max_width - style.padding.left - style.padding.right)
            .min(max_width - style.padding.left - style.padding.right)
            .max(0.0);
        let content_x = outer_x + style.padding.left;
        let content_y = outer_y + style.padding.top;
        let background_index = self.primitives.len();
        let children_height = self.children(id, content_x, content_y, content_width);
        let content_height = style.height.unwrap_or(children_height).max(children_height);
        let border_width = content_width + style.padding.left + style.padding.right;
        let border_height = content_height + style.padding.top + style.padding.bottom;
        if let Some(color) = style.background {
            self.primitives.insert(
                background_index,
                Primitive::Rectangle {
                    x: outer_x,
                    y: outer_y,
                    width: border_width,
                    height: border_height,
                    color,
                },
            );
        }
        style.margin.top + border_height + style.margin.bottom
    }
}

pub fn layout(document: &Document, styles: &[ComputedStyle], viewport_width: f32) -> Scene {
    let mut builder = Builder {
        document,
        styles,
        primitives: Vec::new(),
    };
    let height = builder.children(0, 0.0, 0.0, viewport_width).max(1.0);
    Scene {
        width: viewport_width,
        height,
        primitives: builder.primitives,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{css, html, style};

    #[test]
    fn wraps_text_and_respects_hidden_nodes() {
        let document = html::parse(
            "<style>.hidden { display:none }</style><div>alpha beta gamma</div><p class='hidden'>gone</p>",
        ).unwrap();
        let styles = style::compute(&document, &css::parse(&document.stylesheets()));
        let scene = layout(&document, &styles, 60.0);
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
