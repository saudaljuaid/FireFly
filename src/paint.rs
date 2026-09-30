use std::collections::BTreeMap;
use std::fmt::Write;

use crate::layout::{Primitive, Scene};
use crate::style::{BorderStyle, Color};
use crate::text;

/// The painter independently bounds output even for a manually built Scene.
const MAX_PAINTED_GLYPHS: usize = 200_000;
const MAX_DEFINITION_BYTES: usize = 16 * 1024 * 1024;
const MAX_SVG_BYTES: usize = 128 * 1024 * 1024;
const MAX_TITLE_BYTES: usize = 65_536;

fn escape_xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            c if ((c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r'))
                || matches!(c, '\u{fffe}' | '\u{ffff}') =>
            {
                escaped.push('\u{fffd}')
            }
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

fn glyph_id(face: text::FontFace, glyph: u16) -> String {
    let face = match face {
        text::FontFace::DejaVuRegular => "regular",
        text::FontFace::DejaVuBold => "bold",
        text::FontFace::Cjk => "cjk",
    };
    format!("phos-glyph-{face}-{glyph}")
}

fn title(value: &str) -> String {
    let mut end = value.len().min(MAX_TITLE_BYTES);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    escape_xml(&value[..end])
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
    let mut truncated = scene.truncated;
    writeln!(svg, "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{:.0}\" height=\"{:.0}\" viewBox=\"0 0 {:.2} {:.2}\">",
        scene.width, scene.height.ceil(), scene.width, scene.height.ceil()).unwrap();
    // Glyph outlines are defined once per selected face and glyph ID. Layout
    // supplied all advances and offsets; the SVG viewer performs no shaping,
    // fallback, or bidi reordering. No font binary is copied into the SVG.
    let mut outlines = BTreeMap::new();
    let mut seen_glyphs = 0;
    let mut definition_bytes = 0;
    for primitive in &scene.primitives {
        if let Primitive::Text { run, .. } = primitive {
            for glyph in &run.glyphs {
                if seen_glyphs >= MAX_PAINTED_GLYPHS {
                    truncated = true;
                    break;
                }
                seen_glyphs += 1;
                outlines.entry((run.face, glyph.id)).or_insert_with(|| {
                    let path = text::glyph_outline(run.face, glyph.id)?;
                    if definition_bytes + path.len() > MAX_DEFINITION_BYTES {
                        truncated = true;
                        let replacement = "M50 0V750H550V0ZM100 50H500V700H100Z";
                        if definition_bytes + replacement.len() <= MAX_DEFINITION_BYTES {
                            definition_bytes += replacement.len();
                            Some(replacement.to_owned())
                        } else {
                            None
                        }
                    } else {
                        definition_bytes += path.len();
                        Some(path)
                    }
                });
            }
        }
    }
    svg.push_str("<defs>\n");
    for (&(face, glyph), path) in &outlines {
        if let Some(path) = path {
            writeln!(svg, "<path id=\"{}\" d=\"{path}\"/>", glyph_id(face, glyph)).unwrap();
        }
    }
    svg.push_str("</defs>\n");
    writeln!(
        svg,
        "<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>"
    )
    .unwrap();
    let mut clip_id = 0usize;
    let mut clip_depth = 0usize;
    let mut text_id = 0usize;
    let mut painted_glyphs = 0;
    for primitive in &scene.primitives {
        // Reserve the worst escaped title, primitive syntax, and the closing
        // tags of every open clip. Truncation always leaves well-formed XML.
        if svg.len() + MAX_TITLE_BYTES * 6 + 1024 + clip_depth * 5 >= MAX_SVG_BYTES {
            truncated = true;
            break;
        }
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
                color: ink,
                run,
                node,
                ..
            } => {
                truncated |= run.content.len() > MAX_TITLE_BYTES;
                text_id += 1;
                let direction = match run.direction {
                    text::Direction::Ltr => "ltr",
                    text::Direction::Rtl => "rtl",
                };
                writeln!(svg, "<g data-phos-text=\"true\" data-node=\"{node}\" data-font=\"{}\" data-direction=\"{direction}\" data-advance=\"{:.4}\" role=\"img\" aria-labelledby=\"phos-text-{text_id}\" fill=\"{}\" fill-opacity=\"{:.3}\"><title id=\"phos-text-{text_id}\">{}</title>",
                    run.face.name(), run.advance, color(*ink), opacity(*ink), title(&run.content)).unwrap();
                let scale = run.size / text::units_per_em(run.face);
                for glyph in &run.glyphs {
                    if painted_glyphs >= MAX_PAINTED_GLYPHS
                        || svg.len() + 1024 + clip_depth * 5 >= MAX_SVG_BYTES
                    {
                        truncated = true;
                        break;
                    }
                    painted_glyphs += 1;
                    if outlines
                        .get(&(run.face, glyph.id))
                        .is_some_and(Option::is_some)
                    {
                        let gx = x + glyph.x;
                        let gy = baseline + glyph.y;
                        writeln!(svg, "<use href=\"#{}\" transform=\"translate({gx:.4} {gy:.4}) scale({scale:.8} {:.8})\"/>", glyph_id(run.face, glyph.id), -scale).unwrap();
                    }
                }
                svg.push_str("</g>\n");
            }
            Primitive::Image {
                x,
                y,
                width,
                height,
                href,
            } => {
                let href = escape_xml(href);
                if svg.len() + href.len() + 1024 + clip_depth * 5 >= MAX_SVG_BYTES {
                    truncated = true;
                    continue;
                }
                writeln!(svg, "<image x=\"{x:.2}\" y=\"{y:.2}\" width=\"{width:.2}\" height=\"{height:.2}\" href=\"{href}\" preserveAspectRatio=\"none\"/>").unwrap();
            }
            Primitive::ClipStart {
                x,
                y,
                width,
                height,
                radius,
            } => {
                clip_id += 1;
                clip_depth += 1;
                let path = rounded_path(*x, *y, *width, *height, *radius);
                writeln!(svg, "<defs><clipPath id=\"phos-clip-{clip_id}\"><path d=\"{path}\"/></clipPath></defs><g clip-path=\"url(#phos-clip-{clip_id})\">").unwrap();
            }
            Primitive::ClipEnd if clip_depth > 0 => {
                clip_depth -= 1;
                svg.push_str("</g>\n");
            }
            Primitive::ClipEnd => {}
        }
    }
    for _ in 0..clip_depth {
        svg.push_str("</g>\n");
    }
    svg.push_str("</svg>\n");
    if truncated {
        svg.insert_str(4, " data-phos-truncated=\"true\"");
    }
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
        assert!(svg.contains("data-phos-text=\"true\""));
        assert!(svg.contains("<use href=\"#phos-glyph-"));
        assert!(!svg.contains("<text "));
        let svg = render("<p>\u{fffe}\u{ffff}\u{7}</p>", 500.0).unwrap();
        roxmltree::Document::parse(&svg).unwrap();
        assert!(!svg.contains('\u{fffe}') && !svg.contains('\u{ffff}') && !svg.contains('\u{7}'));
    }

    #[test]
    fn glyph_definitions_are_shared_and_match_resolved_positions() {
        let source = "<style>html,body{margin:0}</style><div>A A</div>";
        let document = crate::html::parse(source).unwrap();
        let styles = crate::style::compute(&document, &crate::css::parse(&document.stylesheets()));
        let scene = crate::layout::layout(&document, &styles, 300.0);
        let svg = super::to_svg(&scene);
        let xml = roxmltree::Document::parse(&svg).unwrap();
        let definitions: Vec<_> = xml
            .descendants()
            .filter(|node| node.has_tag_name("path") && node.attribute("id").is_some())
            .collect();
        assert_eq!(definitions.len(), 1);
        let glyph_uses: Vec<_> = xml
            .descendants()
            .filter(|node| node.has_tag_name("use"))
            .collect();
        assert_eq!(glyph_uses.len(), 2);
        assert_eq!(
            glyph_uses[0].attribute("href"),
            glyph_uses[1].attribute("href")
        );
        let first = &scene.runs[0];
        let glyph = &first.text.glyphs[0];
        let translation = format!(
            "translate({:.4} {:.4})",
            first.x + glyph.x,
            first.baseline + glyph.y
        );
        assert!(
            glyph_uses[0]
                .attribute("transform")
                .unwrap()
                .starts_with(&translation)
        );
    }

    #[test]
    fn scene_truncation_keeps_valid_clip_structure_and_metadata() {
        let source = "<style>div{overflow:hidden;background:red}</style><div>A</div>";
        let document = crate::html::parse(source).unwrap();
        let styles = crate::style::compute(&document, &crate::css::parse(&document.stylesheets()));
        let mut scene = crate::layout::layout(&document, &styles, 300.0);
        scene.truncated = true;
        // Simulate a bounded scene prefix ending inside its ancestor clip.
        while !matches!(
            scene.primitives.last(),
            Some(crate::layout::Primitive::ClipStart { .. })
        ) {
            scene.primitives.pop();
        }
        let svg = super::to_svg(&scene);
        let xml = roxmltree::Document::parse(&svg).unwrap();
        assert_eq!(
            xml.root_element().attribute("data-phos-truncated"),
            Some("true")
        );
        assert_eq!(
            xml.descendants()
                .filter(|node| node.attribute("clip-path").is_some())
                .count(),
            1
        );
    }
}
