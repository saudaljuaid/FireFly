use std::fmt::Write;

use crate::layout::{Primitive, Scene};

fn escape_xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

pub fn to_svg(scene: &Scene) -> String {
    let mut svg = String::new();
    writeln!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{:.0}\" height=\"{:.0}\" viewBox=\"0 0 {:.2} {:.2}\">",
        scene.width,
        scene.height.ceil(),
        scene.width,
        scene.height.ceil()
    )
    .unwrap();
    writeln!(
        svg,
        "<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>"
    )
    .unwrap();
    for primitive in &scene.primitives {
        match primitive {
            Primitive::Rectangle {
                x,
                y,
                width,
                height,
                color,
            } => {
                writeln!(
                    svg,
                    "<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"{width:.2}\" height=\"{height:.2}\" fill=\"#{:02x}{:02x}{:02x}\"/>",
                    color.0, color.1, color.2
                )
                .unwrap();
            }
            Primitive::Text {
                x,
                baseline,
                content,
                size,
                color,
                bold,
            } => {
                writeln!(
                    svg,
                    "<text x=\"{x:.2}\" y=\"{baseline:.2}\" font-family=\"Arial, sans-serif\" font-size=\"{size:.2}\" font-weight=\"{}\" fill=\"#{:02x}{:02x}{:02x}\">{}</text>",
                    if *bold { "bold" } else { "normal" },
                    color.0, color.1, color.2, escape_xml(content)
                )
                .unwrap();
            }
        }
    }
    svg.push_str("</svg>\n");
    svg
}

#[cfg(test)]
mod tests {
    use crate::render;

    #[test]
    fn escapes_untrusted_text_in_svg() {
        let svg = render("<p>&lt;script&gt;&amp;</p>", 500.0).unwrap();
        assert!(svg.contains("&lt;script&gt;"));
        assert!(svg.contains("&amp;"));
        assert!(!svg.contains("<script>"));
    }
}
