use std::fmt::Write;
use std::sync::OnceLock;

use base64::Engine;

use crate::layout::{Primitive, Scene};
use crate::style::{BorderStyle, Color};
use crate::text;

fn escape_xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => escaped.push('\u{fffd}'),
            c => escaped.push(c),
        }
    }
    escaped
}

fn color(color: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.0, color.1, color.2)
}
fn opacity(color: Color) -> f32 {
    color.3 as f32 / 255.0
}

fn font_css() -> &'static str {
    static CSS: OnceLock<String> = OnceLock::new();
    CSS.get_or_init(|| {
        let regular = base64::engine::general_purpose::STANDARD.encode(text::REGULAR);
        let bold = base64::engine::general_purpose::STANDARD.encode(text::BOLD);
        format!("<style>@font-face{{font-family:'Phos DejaVu';src:url(data:font/ttf;base64,{regular}) format('truetype');font-weight:400}}@font-face{{font-family:'Phos DejaVu';src:url(data:font/ttf;base64,{bold}) format('truetype');font-weight:700}}</style>\n")
    })
}

fn rounded_path(x: f32, y: f32, width: f32, height: f32, radius: [f32; 4]) -> String {
    let [tl, tr, br, bl] = radius;
    format!(
        "M {:.2} {:.2} H {:.2} Q {:.2} {:.2} {:.2} {:.2} V {:.2} Q {:.2} {:.2} {:.2} {:.2} H {:.2} Q {:.2} {:.2} {:.2} {:.2} V {:.2} Q {:.2} {:.2} {:.2} {:.2} Z",
        x + tl,
        y,
        x + width - tr,
        x + width,
        y,
        x + width,
        y + tr,
        y + height - br,
        x + width,
        y + height,
        x + width - br,
        y + height,
        x + bl,
        x,
        y + height,
        x,
        y + height - bl,
        y + tl,
        x,
        y,
        x + tl,
        y
    )
}

pub fn to_svg(scene: &Scene) -> String {
    let mut svg = String::new();
    writeln!(svg, "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{:.0}\" height=\"{:.0}\" viewBox=\"0 0 {:.2} {:.2}\">",
        scene.width, scene.height.ceil(), scene.width, scene.height.ceil()).unwrap();
    svg.push_str(font_css());
    writeln!(
        svg,
        "<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>"
    )
    .unwrap();
    let mut clip_id = 0usize;
    for primitive in &scene.primitives {
        match primitive {
            Primitive::Box {
                x,
                y,
                width,
                height,
                background,
                border_color,
                border_width,
                border_style,
                radius,
            } => {
                if *width <= 0.0 || *height <= 0.0 {
                    continue;
                }
                let path = rounded_path(*x, *y, *width, *height, *radius);
                if let Some(background) = background {
                    writeln!(
                        svg,
                        "<path d=\"{path}\" fill=\"{}\" fill-opacity=\"{:.3}\"/>",
                        color(*background),
                        opacity(*background)
                    )
                    .unwrap();
                }
                if *border_style != BorderStyle::None {
                    let [top, right, bottom, left] = *border_width;
                    let stroke = top.max(right).max(bottom).max(left);
                    if stroke > 0.0 {
                        let dash = match border_style {
                            BorderStyle::Dashed => " stroke-dasharray=\"6 4\"",
                            BorderStyle::Dotted => {
                                " stroke-dasharray=\"1 3\" stroke-linecap=\"round\""
                            }
                            _ => "",
                        };
                        if (top - right).abs() < 0.01
                            && (top - bottom).abs() < 0.01
                            && (top - left).abs() < 0.01
                        {
                            let stroke_path = rounded_path(
                                *x + stroke / 2.0,
                                *y + stroke / 2.0,
                                (*width - stroke).max(0.0),
                                (*height - stroke).max(0.0),
                                radius.map(|value| (value - stroke / 2.0).max(0.0)),
                            );
                            writeln!(svg, "<path d=\"{stroke_path}\" fill=\"none\" stroke=\"{}\" stroke-opacity=\"{:.3}\" stroke-width=\"{stroke:.2}\"{dash}/>", color(*border_color), opacity(*border_color)).unwrap();
                        } else {
                            for (bx, by, bw, bh) in [
                                (*x, *y, *width, top),
                                (*x + *width - right, *y, right, *height),
                                (*x, *y + *height - bottom, *width, bottom),
                                (*x, *y, left, *height),
                            ] {
                                if bw > 0.0 && bh > 0.0 {
                                    writeln!(svg, "<rect x=\"{bx:.2}\" y=\"{by:.2}\" width=\"{bw:.2}\" height=\"{bh:.2}\" fill=\"{}\" fill-opacity=\"{:.3}\"/>", color(*border_color), opacity(*border_color)).unwrap();
                                }
                            }
                        }
                    }
                }
            }
            Primitive::Text {
                x,
                baseline,
                content,
                width,
                size,
                color: ink,
                bold,
            } => {
                writeln!(svg, "<text x=\"{x:.2}\" y=\"{baseline:.2}\" font-family=\"Phos DejaVu, DejaVu Sans, Noto Sans CJK SC, Microsoft YaHei, Yu Gothic, SimSun, sans-serif\" font-size=\"{size:.2}\" font-weight=\"{}\" fill=\"{}\" fill-opacity=\"{:.3}\" textLength=\"{width:.2}\" lengthAdjust=\"spacingAndGlyphs\" xml:space=\"preserve\">{}</text>",
                    if *bold { "700" } else { "400" }, color(*ink), opacity(*ink), escape_xml(content)).unwrap();
            }
            Primitive::Image {
                x,
                y,
                width,
                height,
                href,
            } => {
                writeln!(svg, "<image x=\"{x:.2}\" y=\"{y:.2}\" width=\"{width:.2}\" height=\"{height:.2}\" href=\"{}\" preserveAspectRatio=\"none\"/>", escape_xml(href)).unwrap();
            }
            Primitive::ClipStart {
                x,
                y,
                width,
                height,
                radius,
            } => {
                clip_id += 1;
                let path = rounded_path(*x, *y, *width, *height, *radius);
                writeln!(svg, "<defs><clipPath id=\"phos-clip-{clip_id}\"><path d=\"{path}\"/></clipPath></defs><g clip-path=\"url(#phos-clip-{clip_id})\">").unwrap();
            }
            Primitive::ClipEnd => svg.push_str("</g>\n"),
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
        assert!(svg.contains("textLength="));
    }
}
