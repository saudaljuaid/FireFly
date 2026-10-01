use std::collections::BTreeMap;
use std::fmt::Write;

use crate::effects::{BoxShadow, LinearGradient, MAX_BOX_SHADOWS, MAX_GRADIENT_STOPS};
use crate::layout::{Primitive, Scene};
use crate::style::{BorderStyle, Color};
use crate::text;

/// The painter independently bounds output even for a manually built Scene.
const MAX_PAINTED_GLYPHS: usize = 200_000;
const MAX_DEFINITION_BYTES: usize = 16 * 1024 * 1024;
const MAX_SVG_BYTES: usize = 128 * 1024 * 1024;
const MAX_TITLE_BYTES: usize = 65_536;
pub const MAX_EFFECT_DEFINITIONS: usize = 4096;
pub const MAX_SHADOW_SURFACE_PIXELS: f64 = 16_777_216.0;
pub const MAX_TOTAL_SHADOW_SURFACE_PIXELS: f64 = 67_108_864.0;

#[derive(Default)]
struct EffectDefinitions {
    ids: BTreeMap<String, String>,
    exhausted: bool,
    shadow_surface: f64,
}

impl EffectDefinitions {
    fn define(
        &mut self,
        svg: &mut String,
        key: String,
        bytes: &mut usize,
        truncated: &mut bool,
        build: impl FnOnce(&str) -> Option<String>,
    ) -> Option<String> {
        if let Some(id) = self.ids.get(&key) {
            return Some(id.clone());
        }
        if self.exhausted || self.ids.len() >= MAX_EFFECT_DEFINITIONS {
            self.exhausted = true;
            *truncated = true;
            return None;
        }
        let id = format!("phos-effect-{}", self.ids.len() + 1);
        let Some(definition) = build(&id) else {
            *truncated = true;
            return None;
        };
        if *bytes + definition.len() > MAX_DEFINITION_BYTES
            || svg.len() + definition.len() + MAX_TITLE_BYTES * 6 + 2048 >= MAX_SVG_BYTES
        {
            self.exhausted = true;
            *truncated = true;
            return None;
        }
        *bytes += definition.len();
        svg.push_str("<defs>");
        svg.push_str(&definition);
        svg.push_str("</defs>\n");
        self.ids.insert(key, id.clone());
        Some(id)
    }
}

#[derive(Clone, Copy, Debug)]
struct GradientPaintStop {
    offset: f64,
    rgb: [f64; 3],
    alpha: f64,
}

fn premultiplied_stop(a: (f32, Color), b: (f32, Color), fraction: f64) -> GradientPaintStop {
    let aa = f64::from(a.1.3) / 255.0;
    let ba = f64::from(b.1.3) / 255.0;
    let alpha = aa + (ba - aa) * fraction;
    let av = [a.1.0, a.1.1, a.1.2];
    let bv = [b.1.0, b.1.1, b.1.2];
    let rgb = std::array::from_fn(|channel| {
        if alpha > 0.0 {
            (f64::from(av[channel]) * aa * (1.0 - fraction)
                + f64::from(bv[channel]) * ba * fraction)
                / (255.0 * alpha)
        } else if aa > 0.0 {
            f64::from(av[channel]) / 255.0
        } else {
            f64::from(bv[channel]) / 255.0
        }
    });
    GradientPaintStop {
        offset: f64::from(a.0) + f64::from(b.0 - a.0) * fraction,
        rgb,
        alpha,
    }
}

/// SVG interpolates RGB and alpha separately. Split premultiplied CSS intervals
/// until the straight interpolation error is <=1/1024 per premultiplied channel.
/// On a leaf the error is |deltaRGB*deltaAlpha|/4; eight bisections guarantee
/// that bound even for arbitrary endpoint colors. At most 256 leaves/interval.
fn gradient_paint_stops(stops: &[(f32, Color)]) -> Vec<GradientPaintStop> {
    let mut output = Vec::new();
    for (interval, pair) in stops.windows(2).enumerate() {
        let a = pair[0];
        let b = pair[1];
        let start = premultiplied_stop(a, b, 0.0);
        let end = premultiplied_stop(a, b, 1.0);
        if interval == 0 {
            output.push(start);
        }
        if a.0 == b.0 {
            output.push(end);
            continue;
        }
        let mut pending = vec![(0.0, 1.0, start, end, 0)];
        while let Some((low, high, first, last, depth)) = pending.pop() {
            let error = first
                .rgb
                .into_iter()
                .zip(last.rgb)
                .map(|(a, b)| (a - b).abs() * (first.alpha - last.alpha).abs() / 4.0)
                .fold(0.0, f64::max);
            if error <= 1.0 / 1024.0 || depth == 8 {
                output.push(last);
            } else {
                let middle = (low + high) / 2.0;
                let midpoint = premultiplied_stop(a, b, middle);
                pending.push((middle, high, midpoint, last, depth + 1));
                pending.push((low, middle, first, midpoint, depth + 1));
            }
        }
        // A transparent stop can need a different unassociated color on its
        // following interval. Its zero opacity makes this duplicate invisible.
        if interval + 2 < stops.len() && b.1.3 == 0 {
            output.push(premultiplied_stop(b, stops[interval + 2], 0.0));
        }
    }
    output
}

fn paint_gradient(
    svg: &mut String,
    definitions: &mut EffectDefinitions,
    bytes: &mut usize,
    truncated: &mut bool,
    geometry: (f32, f32, f32, f32),
    radius: [f32; 4],
    gradient: &LinearGradient,
) {
    let (x, y, width, height) = geometry;
    if gradient.stops.len() > MAX_GRADIENT_STOPS {
        *truncated = true;
        return;
    }
    let key = format!("gradient:{width:?}:{height:?}:{gradient:?}");
    let id = definitions.define(svg, key, bytes, truncated, |id| {
        let resolved = gradient.resolve(width, height)?;
        let mut definition = format!("<linearGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" x1=\"{:.6}\" y1=\"{:.6}\" x2=\"{:.6}\" y2=\"{:.6}\" color-interpolation=\"sRGB\" spreadMethod=\"pad\">", resolved.start[0], resolved.start[1], resolved.end[0], resolved.end[1]);
        for stop in gradient_paint_stops(&resolved.stops) {
            write!(definition, "<stop offset=\"{:.8}\" stop-color=\"rgb({:.6},{:.6},{:.6})\" stop-opacity=\"{:.8}\"/>", stop.offset, stop.rgb[0] * 255.0, stop.rgb[1] * 255.0, stop.rgb[2] * 255.0, stop.alpha).unwrap();
        }
        definition.push_str("</linearGradient>");
        Some(definition)
    });
    if let Some(id) = id {
        let path = rounded_path(0.0, 0.0, width, height, radius);
        writeln!(svg, "<g transform=\"translate({x:.4} {y:.4})\"><path data-phos-gradient=\"true\" d=\"{path}\" fill=\"url(#{id})\"/></g>").unwrap();
    }
}

fn paint_shadow(
    svg: &mut String,
    definitions: &mut EffectDefinitions,
    bytes: &mut usize,
    truncated: &mut bool,
    geometry: (f32, f32, f32, f32),
    radius: [f32; 4],
    shadow: BoxShadow,
) {
    let (x, y, width, height) = geometry;
    if shadow.color.3 == 0 {
        return;
    }
    if [shadow.offset_x, shadow.offset_y, shadow.blur, shadow.spread]
        .into_iter()
        .any(|v| !v.is_finite())
        || shadow.offset_x.abs() > crate::effects::MAX_SHADOW_OFFSET
        || shadow.offset_y.abs() > crate::effects::MAX_SHADOW_OFFSET
        || shadow.spread.abs() > crate::effects::MAX_SHADOW_OFFSET
        || !(0.0..=crate::effects::MAX_SHADOW_BLUR).contains(&shadow.blur)
    {
        *truncated = true;
        return;
    }
    let sw = (width + shadow.spread * 2.0).max(0.0);
    let sh = (height + shadow.spread * 2.0).max(0.0);
    if sw <= 0.0 || sh <= 0.0 {
        return;
    }
    let bounds = shadow.bounds(0.0, 0.0, width, height);
    let left = bounds[0].min(0.0);
    let top = bounds[1].min(0.0);
    let right = bounds[2].max(width);
    let bottom = bounds[3].max(height);
    let area = f64::from(right - left) * f64::from(bottom - top);
    if !area.is_finite()
        || area > MAX_SHADOW_SURFACE_PIXELS
        || definitions.shadow_surface + area > MAX_TOTAL_SHADOW_SURFACE_PIXELS
    {
        *truncated = true;
        return;
    }
    definitions.shadow_surface += area;
    let mask_key = format!(
        "shadow-mask:{width:?}:{height:?}:{radius:?}:{left:?}:{top:?}:{right:?}:{bottom:?}"
    );
    let mask = definitions.define(svg, mask_key, bytes, truncated, |id| {
        let knockout = rounded_path(0.0, 0.0, width, height, radius);
        Some(format!("<mask id=\"{id}\" maskUnits=\"userSpaceOnUse\" maskContentUnits=\"userSpaceOnUse\" x=\"{left:.4}\" y=\"{top:.4}\" width=\"{:.4}\" height=\"{:.4}\" style=\"mask-type:luminance\"><rect x=\"{left:.4}\" y=\"{top:.4}\" width=\"{:.4}\" height=\"{:.4}\" fill=\"white\"/><path d=\"{knockout}\" fill=\"black\"/></mask>", right-left, bottom-top, right-left, bottom-top))
    });
    let Some(mask) = mask else {
        return;
    };
    let filter = if shadow.blur > 0.0 {
        let key = format!("shadow-filter:{bounds:?}:{:?}", shadow.blur);
        let filter = definitions.define(svg, key, bytes, truncated, |id| Some(format!(
            "<filter id=\"{id}\" filterUnits=\"userSpaceOnUse\" primitiveUnits=\"userSpaceOnUse\" x=\"{:.4}\" y=\"{:.4}\" width=\"{:.4}\" height=\"{:.4}\" color-interpolation-filters=\"sRGB\"><feGaussianBlur in=\"SourceGraphic\" stdDeviation=\"{:.4}\"/></filter>", bounds[0], bounds[1], bounds[2]-bounds[0], bounds[3]-bounds[1], shadow.blur/2.0)));
        let Some(filter) = filter else {
            return;
        };
        format!(" filter=\"url(#{filter})\"")
    } else {
        String::new()
    };
    let path = rounded_path(
        shadow.offset_x - shadow.spread,
        shadow.offset_y - shadow.spread,
        sw,
        sh,
        shadow.spread_radii(radius, sw, sh),
    );
    writeln!(svg, "<g transform=\"translate({x:.4} {y:.4})\" mask=\"url(#{mask})\"><path data-phos-shadow=\"true\" d=\"{path}\" fill=\"{}\" fill-opacity=\"{:.8}\"{filter}/></g>", color(shadow.color), opacity(shadow.color)).unwrap();
}

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
        "M {:.2} {:.2} H {:.2} A {tr:.2} {tr:.2} 0 0 1 {:.2} {:.2} V {:.2} A {br:.2} {br:.2} 0 0 1 {:.2} {:.2} H {:.2} A {bl:.2} {bl:.2} 0 0 1 {:.2} {:.2} V {:.2} A {tl:.2} {tl:.2} 0 0 1 {:.2} {:.2} Z",
        x + tl,
        y,
        x + width - tr,
        x + width,
        y + tr,
        y + height - br,
        x + width - br,
        y + height,
        x + bl,
        x,
        y + height - bl,
        y + tl,
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
    let mut effects = EffectDefinitions::default();
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
            }
            | Primitive::DecoratedBox {
                x,
                y,
                width,
                height,
                background,
                border_color,
                border_width,
                border_style,
                radius,
                ..
            } => {
                if *width <= 0.0 || *height <= 0.0 {
                    continue;
                }
                let path = rounded_path(*x, *y, *width, *height, *radius);
                if let Primitive::DecoratedBox { shadows, .. } = primitive {
                    truncated |= shadows.len() > MAX_BOX_SHADOWS;
                    // CSS's first authored shadow is foremost. All outer
                    // shadows precede the element's own background and clip.
                    for shadow in shadows.iter().take(MAX_BOX_SHADOWS).rev() {
                        paint_shadow(
                            &mut svg,
                            &mut effects,
                            &mut definition_bytes,
                            &mut truncated,
                            (*x, *y, *width, *height),
                            *radius,
                            *shadow,
                        );
                    }
                }
                if let Some(background) = background {
                    writeln!(
                        svg,
                        "<path d=\"{path}\" fill=\"{}\" fill-opacity=\"{:.3}\"/>",
                        color(*background),
                        opacity(*background)
                    )
                    .unwrap();
                }
                if let Primitive::DecoratedBox {
                    gradient: Some(gradient),
                    ..
                } = primitive
                {
                    paint_gradient(
                        &mut svg,
                        &mut effects,
                        &mut definition_bytes,
                        &mut truncated,
                        (*x, *y, *width, *height),
                        *radius,
                        gradient,
                    );
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
    #[test]
    fn rounded_boxes_use_exact_circular_arcs_in_all_four_corners() {
        let path = super::rounded_path(10.0, 20.0, 100.0, 60.0, [4.0, 8.0, 12.0, 16.0]);
        assert_eq!(
            path,
            "M 14.00 20.00 H 102.00 A 8.00 8.00 0 0 1 110.00 28.00 V 68.00 A 12.00 12.00 0 0 1 98.00 80.00 H 26.00 A 16.00 16.00 0 0 1 10.00 64.00 V 24.00 A 4.00 4.00 0 0 1 14.00 20.00 Z"
        );
    }

    use crate::render;

    #[test]
    fn premultiplied_gradient_samples_bound_straight_svg_interpolation_error() {
        use crate::style::Color;
        for (a, b) in [
            (Color(255, 0, 0, 51), Color(0, 0, 255, 230)),
            (Color(255, 128, 0, 1), Color(0, 255, 128, 255)),
            (Color(0, 0, 0, 0), Color(255, 0, 0, 255)),
            (Color(0, 0, 255, 255), Color(0, 0, 0, 0)),
            (Color(255, 0, 0, 0), Color(0, 0, 255, 0)),
        ] {
            let stops = super::gradient_paint_stops(&[(0.0, a), (1.0, b)]);
            assert!(stops.len() <= 257);
            for pair in stops.windows(2) {
                let first = pair[0];
                let last = pair[1];
                let fraction = (first.offset + last.offset) / 2.0;
                let expected = super::premultiplied_stop((0.0, a), (1.0, b), fraction);
                let alpha = (first.alpha + last.alpha) / 2.0;
                for channel in 0..3 {
                    let actual = (first.rgb[channel] + last.rgb[channel]) / 2.0 * alpha;
                    assert!(
                        (actual - expected.rgb[channel] * expected.alpha).abs()
                            <= 1.0 / 1024.0 + 0.000_000_1
                    );
                }
            }
        }
    }

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
