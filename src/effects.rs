//! Bounded static gradients and outer shadows. No external resources are used.
//!
//! Length parsing is delegated to the engine's shared values parser. Stops keep
//! unresolved percentages until the gradient line is known; shadow lengths must
//! be definite. Invalid complete values do not partially replace prior CSS.

use crate::style::{Color, Length};
use crate::values::{comma_components, components};

pub const MAX_GRADIENT_STOPS: usize = 16;
pub const MAX_BOX_SHADOWS: usize = 4;
pub const MAX_EFFECT_VALUE_BYTES: usize = 4096;
pub const MAX_SHADOW_OFFSET: f32 = 4096.0;
pub const MAX_SHADOW_BLUR: f32 = 256.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GradientDirection {
    /// CSS degrees: zero is up; positive angles rotate clockwise.
    Angle(f32),
    /// The magic-corner direction is box dependent, unlike a fixed angle.
    Corner { horizontal: i8, vertical: i8 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColorStop {
    pub color: Color,
    pub position: Option<Length>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinearGradient {
    pub direction: GradientDirection,
    pub stops: Vec<ColorStop>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedGradient {
    /// Relative to the physical gradient box's top-left edge. The line expands
    /// to cover authored stops outside 0..100%, preserving their transitions.
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub stops: Vec<(f32, Color)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
}

fn direction(value: &str) -> Option<GradientDirection> {
    let value = value.trim();
    if let Some(angle) = value
        .strip_suffix("deg")
        .or_else(|| (value == "0").then_some(value))
    {
        let angle: f32 = angle.parse().ok()?;
        return (angle.is_finite() && angle.abs() <= 16_384.0)
            .then_some(GradientDirection::Angle(angle.rem_euclid(360.0)));
    }
    let parts = components(value)?;
    if parts.first().copied()? != "to" || !(2..=3).contains(&parts.len()) {
        return None;
    }
    let mut horizontal = 0;
    let mut vertical = 0;
    for part in &parts[1..] {
        match *part {
            "left" if horizontal == 0 => horizontal = -1,
            "right" if horizontal == 0 => horizontal = 1,
            "top" if vertical == 0 => vertical = -1,
            "bottom" if vertical == 0 => vertical = 1,
            _ => return None,
        }
    }
    Some(if horizontal != 0 && vertical != 0 {
        GradientDirection::Corner {
            horizontal,
            vertical,
        }
    } else {
        GradientDirection::Angle(match (horizontal, vertical) {
            (0, -1) => 0.0,
            (1, 0) => 90.0,
            (0, 1) => 180.0,
            (-1, 0) => 270.0,
            _ => return None,
        })
    })
}

pub fn parse_gradient(
    value: &str,
    length: &mut impl FnMut(&str) -> Option<Length>,
) -> Option<LinearGradient> {
    if value.len() > MAX_EFFECT_VALUE_BYTES {
        return None;
    }
    let lower = value.trim().to_ascii_lowercase();
    let source = lower.strip_prefix("linear-gradient(")?.strip_suffix(')')?;
    let parts = comma_components(source)?;
    let explicit = parts.first().and_then(|value| direction(value));
    let stop_parts = &parts[usize::from(explicit.is_some())..];
    if !(2..=MAX_GRADIENT_STOPS).contains(&stop_parts.len()) {
        return None;
    }
    let mut stops = Vec::with_capacity(stop_parts.len());
    for stop in stop_parts {
        let components = components(stop)?;
        if !(1..=2).contains(&components.len()) {
            return None;
        }
        let color = Color::parse(components[0])?;
        let position = if let Some(value) = components.get(1) {
            let position = length(value)?;
            if matches!(
                position,
                Length::Auto | Length::MinContent | Length::MaxContent
            ) || !position.resolve(100.0)?.is_finite()
            {
                return None;
            }
            Some(position)
        } else {
            None
        };
        stops.push(ColorStop { color, position });
    }
    Some(LinearGradient {
        direction: explicit.unwrap_or(GradientDirection::Angle(180.0)),
        stops,
    })
}

pub fn parse_shadows(
    value: &str,
    current_color: Color,
    length: &mut impl FnMut(&str) -> Option<Length>,
) -> Option<Vec<BoxShadow>> {
    if value.len() > MAX_EFFECT_VALUE_BYTES {
        return None;
    }
    let lower = value.trim().to_ascii_lowercase();
    if lower == "none" {
        return Some(Vec::new());
    }
    let layers = comma_components(&lower)?;
    if layers.is_empty() || layers.len() > MAX_BOX_SHADOWS {
        return None;
    }
    let mut shadows = Vec::with_capacity(layers.len());
    for layer in layers {
        let parts = components(layer)?;
        let mut color = None;
        let mut lengths = Vec::with_capacity(4);
        for part in parts {
            if part == "inset" {
                return None;
            }
            if part == "currentcolor" || Color::parse(part).is_some() {
                if color.is_some() {
                    return None;
                }
                color = Some(if part == "currentcolor" {
                    current_color
                } else {
                    Color::parse(part)?
                });
            } else {
                if lengths.len() == 4 {
                    return None;
                }
                let value = length(part)?.resolve_indefinite(None)?;
                if !value.is_finite() || value.abs() > MAX_SHADOW_OFFSET {
                    return None;
                }
                lengths.push(value);
            }
        }
        if !(2..=4).contains(&lengths.len()) {
            return None;
        }
        let blur = lengths.get(2).copied().unwrap_or(0.0);
        if !(0.0..=MAX_SHADOW_BLUR).contains(&blur) {
            return None;
        }
        shadows.push(BoxShadow {
            offset_x: lengths[0],
            offset_y: lengths[1],
            blur,
            spread: lengths.get(3).copied().unwrap_or(0.0),
            color: color.unwrap_or(current_color),
        });
    }
    Some(shadows)
}

impl LinearGradient {
    pub fn resolve(&self, width: f32, height: f32) -> Option<ResolvedGradient> {
        if !(2..=MAX_GRADIENT_STOPS).contains(&self.stops.len())
            || !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
        {
            return None;
        }
        let (dx, dy) = match self.direction {
            GradientDirection::Angle(angle) if angle.is_finite() => {
                let radians = f64::from(angle).to_radians();
                (radians.sin(), -radians.cos())
            }
            GradientDirection::Corner {
                horizontal,
                vertical,
            } if matches!(horizontal, -1 | 1) && matches!(vertical, -1 | 1) => {
                // A line perpendicular to the neighboring-corner diagonal.
                let magnitude = f64::from(width).hypot(f64::from(height));
                (
                    f64::from(horizontal) * f64::from(height) / magnitude,
                    f64::from(vertical) * f64::from(width) / magnitude,
                )
            }
            _ => return None,
        };
        let line = f64::from(width) * dx.abs() + f64::from(height) * dy.abs();
        if line <= 0.0 || !line.is_finite() {
            return None;
        }
        let mut positions: Vec<Option<f64>> = self
            .stops
            .iter()
            .map(|stop| {
                stop.position
                    .and_then(|position| position.resolve(line as f32))
                    .map(|position| f64::from(position) / line)
            })
            .collect();
        for (stop, position) in self.stops.iter().zip(&positions) {
            if stop.position.is_some() && position.is_none_or(|value| !value.is_finite()) {
                return None;
            }
        }
        if positions[0].is_none() {
            positions[0] = Some(0.0);
        }
        let last = positions.len() - 1;
        if positions[last].is_none() {
            positions[last] = Some(1.0);
        }
        let mut largest = f64::NEG_INFINITY;
        for position in positions.iter_mut().flatten() {
            *position = position.max(largest);
            largest = *position;
        }
        let mut before = 0;
        while before < last {
            let mut after = before + 1;
            while positions[after].is_none() {
                after += 1;
            }
            let start = positions[before]?;
            let step = (positions[after]? - start) / (after - before) as f64;
            for (offset, position) in positions[before + 1..after].iter_mut().enumerate() {
                *position = Some(start + step * (offset + 1) as f64);
            }
            before = after;
        }
        let first = positions[0]?;
        let last_position = positions[last]?;
        let (start_position, end_position) = if last_position > first {
            (first, last_position)
        } else {
            // Equal stops are a hard transition, not a degenerate solid fill.
            (first - 0.5, first + 0.5)
        };
        let raw_start = [
            f64::from(width) / 2.0 - dx * line / 2.0,
            f64::from(height) / 2.0 - dy * line / 2.0,
        ];
        let point = |position: f64| {
            [
                (raw_start[0] + dx * line * position) as f32,
                (raw_start[1] + dy * line * position) as f32,
            ]
        };
        let start = point(start_position);
        let end = point(end_position);
        if start.into_iter().chain(end).any(|value| !value.is_finite()) {
            return None;
        }
        let span = end_position - start_position;
        let stops = positions
            .into_iter()
            .zip(&self.stops)
            .map(|(position, stop)| {
                (
                    ((position.unwrap() - start_position) / span) as f32,
                    stop.color,
                )
            })
            .collect();
        Some(ResolvedGradient { start, end, stops })
    }
}

impl BoxShadow {
    /// Ink bounds including a deliberate three-sigma Gaussian support region.
    /// The omitted tail is below 0.3% of an opaque straight edge. No shadow
    /// changes layout dimensions; SVG filters and scene review use these bounds.
    pub fn bounds(self, x: f32, y: f32, width: f32, height: f32) -> [f32; 4] {
        let support = self.blur * 1.5;
        let left = x + self.offset_x - self.spread;
        let top = y + self.offset_y - self.spread;
        [
            left - support,
            top - support,
            left + (width + self.spread * 2.0).max(0.0) + support,
            top + (height + self.spread * 2.0).max(0.0) + support,
        ]
    }

    pub fn spread_radii(self, radius: [f32; 4], width: f32, height: f32) -> [f32; 4] {
        let radius = radius.map(|radius| {
            let addition = if self.spread > radius && self.spread > 0.0 {
                self.spread * (1.0 + (radius / self.spread - 1.0).powi(3))
            } else {
                self.spread
            };
            (radius + addition).max(0.0)
        });
        let mut scale: f32 = 1.0;
        for (size, sum) in [
            (width, radius[0] + radius[1]),
            (width, radius[3] + radius[2]),
            (height, radius[0] + radius[3]),
            (height, radius[1] + radius[2]),
        ] {
            if sum > 0.0 {
                scale = scale.min(size.max(0.0) / sum);
            }
        }
        radius.map(|value| value * scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn length(value: &str) -> Option<Length> {
        crate::values::parse_length(
            value,
            &crate::values::LengthContext::new(16.0, 16.0, crate::values::Viewport::default()),
            false,
            true,
        )
    }
    fn gradient(value: &str) -> LinearGradient {
        parse_gradient(value, &mut length).unwrap()
    }
    fn close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
    }

    #[test]
    fn css_angles_and_magic_corners_have_correct_line_geometry() {
        let horizontal = gradient("linear-gradient(90deg, red, blue)")
            .resolve(200.0, 100.0)
            .unwrap();
        close(horizontal.start[0], 0.0);
        close(horizontal.end[0], 200.0);
        close(horizontal.start[1], 50.0);
        close(horizontal.end[1], 50.0);
        let corner = gradient("linear-gradient(to top right, red, blue)")
            .resolve(200.0, 100.0)
            .unwrap();
        close(
            (corner.end[0] - corner.start[0]) / (corner.end[1] - corner.start[1]),
            -0.5,
        );
        let default = gradient("linear-gradient(red, blue)")
            .resolve(200.0, 100.0)
            .unwrap();
        close(default.start[1], 0.0);
        close(default.end[1], 100.0);
    }

    #[test]
    fn omitted_and_reversed_stops_follow_css_fixup() {
        let resolved = gradient("linear-gradient(red 80px, white 0px, green, blue 100px)")
            .resolve(100.0, 100.0)
            .unwrap();
        assert_eq!(
            resolved.stops.iter().map(|stop| stop.0).collect::<Vec<_>>(),
            [0.0, 0.0, 0.5, 1.0]
        );
        close(resolved.start[1], 80.0);
        close(resolved.end[1], 100.0);
    }

    #[test]
    fn outside_stops_expand_line_instead_of_clamping_positions() {
        let resolved = gradient("linear-gradient(to right, red -50%, white, blue 150%)")
            .resolve(100.0, 100.0)
            .unwrap();
        close(resolved.start[0], -50.0);
        close(resolved.end[0], 150.0);
        assert_eq!(
            resolved.stops.iter().map(|stop| stop.0).collect::<Vec<_>>(),
            [0.0, 0.5, 1.0]
        );
        let hard = gradient("linear-gradient(red 50%, blue 50%)")
            .resolve(100.0, 100.0)
            .unwrap();
        assert_eq!(hard.stops[0].0, 0.5);
        assert_eq!(hard.stops[1].0, 0.5);
    }

    #[test]
    fn functional_colors_calc_positions_and_malformed_values_are_bounded() {
        let gradient =
            gradient("linear-gradient(-45deg, rgba(0, 0, 255, .5) calc(10px + 5%), transparent)");
        assert_eq!(gradient.stops[0].color.3, 128);
        assert!(gradient.resolve(100.0, 100.0).is_some());
        for invalid in [
            "linear-gradient(red)",
            "linear-gradient(to left right, red, blue)",
            "linear-gradient(NaNdeg, red, blue)",
            "linear-gradient(red auto, blue)",
            "linear-gradient(red 10% 20%, blue)",
            "linear-gradient(red, 20%, blue)",
            "repeating-linear-gradient(red, blue)",
            "linear-gradient(red,,blue)",
        ] {
            assert!(parse_gradient(invalid, &mut length).is_none(), "{invalid}");
        }
        assert!(
            parse_gradient(
                &format!("linear-gradient({})", ["red"; 17].join(",")),
                &mut length
            )
            .is_none()
        );
    }

    #[test]
    fn shadow_colors_lengths_spread_and_limits_are_explicit() {
        let shadows = parse_shadows(
            "2px -3px 10px 4px rgba(0,0,0,.5), red 0 1px",
            Color::WHITE,
            &mut length,
        )
        .unwrap();
        assert_eq!(shadows.len(), 2);
        assert_eq!(shadows[0].blur, 10.0);
        assert_eq!(shadows[0].color.3, 128);
        assert_eq!(shadows[1].spread, 0.0);
        assert_eq!(
            parse_shadows("0 0", Color::WHITE, &mut length).unwrap()[0].color,
            Color::WHITE
        );
        for invalid in [
            "0 0 inset",
            "0 0 -1px",
            "0 0 257px",
            "0 0 1px 5000px",
            "1% 0",
            "0 0 red blue",
            "NaNpx 0",
            "0 0 0 0 0",
            "0 0,",
        ] {
            assert!(
                parse_shadows(invalid, Color::BLACK, &mut length).is_none(),
                "{invalid}"
            );
        }
        assert!(parse_shadows(&["0 0"; 5].join(","), Color::BLACK, &mut length).is_none());
    }

    #[test]
    fn shadow_ink_bounds_and_small_radius_spread_follow_spec() {
        let shadow = BoxShadow {
            offset_x: 2.0,
            offset_y: 3.0,
            blur: 10.0,
            spread: 20.0,
            color: Color::BLACK,
        };
        assert_eq!(
            shadow.bounds(0.0, 0.0, 100.0, 100.0),
            [-33.0, -32.0, 137.0, 138.0]
        );
        assert_eq!(
            shadow.spread_radii([10.0, 0.0, 10.0, 0.0], 140.0, 140.0),
            [27.5, 0.0, 27.5, 0.0]
        );
    }
}
