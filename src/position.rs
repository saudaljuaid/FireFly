//! Bounded physical positioning for the horizontal static layout subset.
//!
//! All percentages resolve against the complete containing padding rectangle,
//! before insets reduce the space available to an absolute box. Flow layout
//! supplies intrinsic width and final border-box size; these helpers never
//! traverse the DOM or alter the space reserved by a relatively positioned box.

use crate::dom::NodeId;
use crate::style::{BorderStyle, BoxSizing, ComputedStyle, Direction, Length, Position};

pub const MAX_POSITION_COORD: f32 = 1_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContainingBlock {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    /// Indefinite initial rectangles leave vertical percentages unresolved.
    /// A positioned ancestor can supply its final in-flow padding-box height.
    pub height: Option<f32>,
}

/// Exact rectangles recorded by block layout, including its final auto height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutAnchor {
    pub node: NodeId,
    pub padding: ContainingBlock,
    pub content_origin: (f32, f32),
    pub content_width: f32,
}

impl LayoutAnchor {
    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.padding.x = coordinate(self.padding.x + dx);
        self.padding.y = coordinate(self.padding.y + dy);
        self.content_origin.0 = coordinate(self.content_origin.0 + dx);
        self.content_origin.1 = coordinate(self.content_origin.1 + dy);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RelativeOffset {
    pub dx: f32,
    pub dy: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AbsoluteSize {
    pub content_width: Option<f32>,
    pub content_height: Option<f32>,
    pub available_width: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AbsoluteOrigin {
    /// Margin-box origin, consistent with the origin consumed by block layout.
    pub x: f32,
    pub y: f32,
    pub margins: [f32; 4],
}

impl AbsoluteOrigin {
    pub fn border_origin(self) -> (f32, f32) {
        (
            coordinate(self.x + self.margins[3]),
            coordinate(self.y + self.margins[0]),
        )
    }
}

fn coordinate(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-MAX_POSITION_COORD, MAX_POSITION_COORD)
    } else {
        0.0
    }
}

fn dimension(value: f32) -> f32 {
    coordinate(value).max(0.0)
}

fn resolve(length: Length, base: Option<f32>) -> Option<f32> {
    let value = match length {
        Length::Px(value) => value,
        Length::Percent(value) => value * base? / 100.0,
        Length::Auto => return None,
    };
    value.is_finite().then_some(coordinate(value))
}

fn dimensions(cb: ContainingBlock) -> (f32, Option<f32>) {
    (
        dimension(cb.width),
        cb.height.filter(|height| height.is_finite()).map(dimension),
    )
}

/// Translate a relative box after layout. Start-side precedence follows the
/// containing block's direction; top wins over bottom in horizontal writing.
pub fn relative_offset(
    style: &ComputedStyle,
    cb_width: f32,
    cb_height: Option<f32>,
    cb_direction: Direction,
) -> RelativeOffset {
    if style.position != Position::Relative {
        return RelativeOffset { dx: 0.0, dy: 0.0 };
    }
    let width = Some(dimension(cb_width));
    let height = cb_height.filter(|height| height.is_finite()).map(dimension);
    let left = resolve(style.insets.left, width);
    let right = resolve(style.insets.right, width);
    let top = resolve(style.insets.top, height);
    let bottom = resolve(style.insets.bottom, height);
    let dx = match (left, right, cb_direction) {
        (_, Some(right), Direction::Rtl) => -right,
        (Some(left), _, _) => left,
        (_, Some(right), _) => -right,
        _ => 0.0,
    };
    RelativeOffset {
        dx: coordinate(dx),
        dy: coordinate(top.unwrap_or_else(|| -bottom.unwrap_or(0.0))),
    }
}

fn box_insets(style: &ComputedStyle, width: f32) -> (f32, f32) {
    let edges = |values: crate::style::Edges| {
        [values.top, values.right, values.bottom, values.left]
            .map(|length| dimension(resolve(length, Some(width)).unwrap_or(0.0)))
    };
    let padding = edges(style.padding);
    let border = if style.border_style == BorderStyle::None {
        [0.0; 4]
    } else {
        edges(style.border_width)
    };
    (
        dimension(padding[1] + padding[3] + border[1] + border[3]),
        dimension(padding[0] + padding[2] + border[0] + border[2]),
    )
}

fn content_size(value: f32, inset: f32, sizing: BoxSizing) -> f32 {
    dimension(if sizing == BoxSizing::BorderBox {
        value - inset
    } else {
        value
    })
}

fn constrain(
    value: f32,
    min: Option<Length>,
    max: Option<Length>,
    base: Option<f32>,
    inset: f32,
    sizing: BoxSizing,
) -> f32 {
    let mut value = dimension(value);
    if let Some(maximum) = max.and_then(|length| resolve(length, base)) {
        value = value.min(content_size(maximum, inset, sizing));
    }
    // CSS minimum constraints win when minimum and maximum conflict.
    if let Some(minimum) = min.and_then(|length| resolve(length, base)) {
        value = value.max(content_size(minimum, inset, sizing));
    }
    dimension(value)
}

/// Resolve explicit sizes and opposing-inset stretch. One-sided/all-auto
/// widths use a bounded preferred-width shrink approximation supplied by the
/// existing intrinsic measurer; heights with auto insets remain content-sized.
pub fn absolute_size(
    style: &ComputedStyle,
    cb: ContainingBlock,
    intrinsic_width: f32,
) -> AbsoluteSize {
    let (width, height) = dimensions(cb);
    let (horizontal, vertical) = box_insets(style, width);
    let left = resolve(style.insets.left, Some(width));
    let right = resolve(style.insets.right, Some(width));
    let top = resolve(style.insets.top, height);
    let bottom = resolve(style.insets.bottom, height);
    let margin = |length| resolve(length, Some(width)).unwrap_or(0.0);
    let available_width = dimension(width - left.unwrap_or(0.0) - right.unwrap_or(0.0));
    let available_content = dimension(
        available_width - horizontal - margin(style.margin.left) - margin(style.margin.right),
    );
    let preferred = style
        .width
        .and_then(|length| resolve(length, Some(width)))
        .map(|value| content_size(value, horizontal, style.box_sizing))
        .unwrap_or_else(|| {
            if left.is_some() && right.is_some() {
                available_content
            } else {
                dimension(intrinsic_width).min(available_content)
            }
        });
    let content_width = Some(constrain(
        preferred,
        style.min_width,
        style.max_width,
        Some(width),
        horizontal,
        style.box_sizing,
    ));
    let content_height = style
        .height
        .and_then(|length| resolve(length, height))
        .map(|value| content_size(value, vertical, style.box_sizing))
        .or_else(|| {
            height.zip(top.zip(bottom)).map(|(height, (top, bottom))| {
                dimension(
                    height
                        - top
                        - bottom
                        - vertical
                        - margin(style.margin.top)
                        - margin(style.margin.bottom),
                )
            })
        })
        .map(|value| {
            constrain(
                value,
                style.min_height,
                style.max_height,
                height,
                vertical,
                style.box_sizing,
            )
        });
    AbsoluteSize {
        content_width,
        content_height,
        available_width,
    }
}

fn axis_margins(
    leading: Option<f32>,
    trailing: Option<f32>,
    remaining: Option<f32>,
    direction: Option<Direction>,
) -> (f32, f32) {
    let Some(remaining) = remaining else {
        return (leading.unwrap_or(0.0), trailing.unwrap_or(0.0));
    };
    let margins = match (leading, trailing) {
        (None, None) if remaining < 0.0 && direction == Some(Direction::Ltr) => (0.0, remaining),
        (None, None) if remaining < 0.0 && direction == Some(Direction::Rtl) => (remaining, 0.0),
        (None, None) => (remaining / 2.0, remaining / 2.0),
        (None, Some(trailing)) => (remaining - trailing, trailing),
        (Some(leading), None) => (leading, remaining - leading),
        (Some(leading), Some(trailing)) => (leading, trailing),
    };
    (coordinate(margins.0), coordinate(margins.1))
}

/// Place a resolved absolute border box. `static_origin` is a bounded normal
/// flow cursor in canvas coordinates and is used when both axis insets are auto.
/// Auto margins are solved only when both opposing insets resolve; otherwise
/// they become zero. Painting translates the final subtree to `border_origin()`.
pub fn absolute_origin(
    style: &ComputedStyle,
    cb: ContainingBlock,
    static_origin: (f32, f32),
    border_size: (f32, f32),
    cb_direction: Direction,
) -> AbsoluteOrigin {
    let (width, height) = dimensions(cb);
    let box_width = dimension(border_size.0);
    let box_height = dimension(border_size.1);
    let left = resolve(style.insets.left, Some(width));
    let right = resolve(style.insets.right, Some(width));
    let top = resolve(style.insets.top, height);
    let bottom = resolve(style.insets.bottom, height);
    let margins = [
        style.margin.top,
        style.margin.right,
        style.margin.bottom,
        style.margin.left,
    ]
    .map(|length| resolve(length, Some(width)));
    let (ml, mr) = axis_margins(
        margins[3],
        margins[1],
        left.zip(right)
            .map(|(left, right)| width - left - right - box_width),
        Some(cb_direction),
    );
    let (mt, mb) = axis_margins(
        margins[0],
        margins[2],
        height
            .zip(top.zip(bottom))
            .map(|(height, (top, bottom))| height - top - bottom - box_height),
        None,
    );
    let x = match (left, right, cb_direction) {
        (_, Some(right), Direction::Rtl) => cb.x + width - right - box_width - ml - mr,
        (Some(left), _, _) => cb.x + left,
        (_, Some(right), _) => cb.x + width - right - box_width - ml - mr,
        _ => static_origin.0,
    };
    let y = if let Some(top) = top {
        cb.y + top
    } else if let Some((height, bottom)) = height.zip(bottom) {
        cb.y + height - bottom - box_height - mt - mb
    } else {
        static_origin.1
    };
    AbsoluteOrigin {
        x: coordinate(x),
        y: coordinate(y),
        margins: [mt, mr, mb, ml],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{css, dom::NodeKind, html, style};

    fn computed(value: &str) -> ComputedStyle {
        let document = html::parse(&format!("<div id=target style='{value}'></div>")).unwrap();
        let styles = style::compute(&document, &css::Stylesheet::default());
        let id = document.nodes.iter().position(|node| matches!(&node.kind, NodeKind::Element(element) if element.attribute("id") == Some("target"))).unwrap();
        styles[id].clone()
    }

    fn cb() -> ContainingBlock {
        ContainingBlock {
            x: 20.0,
            y: 30.0,
            width: 200.0,
            height: Some(100.0),
        }
    }

    #[test]
    fn relative_offsets_preserve_start_side_precedence_and_indefinite_height() {
        let style = computed("position:relative;left:10%;right:5px;top:20%;bottom:9px");
        assert_eq!(
            relative_offset(&style, 200.0, Some(100.0), Direction::Ltr),
            RelativeOffset { dx: 20.0, dy: 20.0 }
        );
        assert_eq!(
            relative_offset(&style, 200.0, Some(100.0), Direction::Rtl),
            RelativeOffset { dx: -5.0, dy: 20.0 }
        );
        assert_eq!(
            relative_offset(&style, 200.0, None, Direction::Ltr),
            RelativeOffset { dx: 20.0, dy: -9.0 }
        );
        assert_eq!(
            relative_offset(
                &computed("left:50px;top:60px"),
                200.0,
                Some(100.0),
                Direction::Ltr
            ),
            RelativeOffset { dx: 0.0, dy: 0.0 }
        );
    }

    #[test]
    fn absolute_percentages_use_full_padding_rectangle_before_insets() {
        let style = computed(
            "position:absolute;left:10%;right:20%;top:10%;bottom:20%;padding:5%;border:2px solid;margin:1%;box-sizing:border-box",
        );
        let size = absolute_size(&style, cb(), 180.0);
        assert_eq!(
            size,
            AbsoluteSize {
                content_width: Some(112.0),
                content_height: Some(42.0),
                available_width: 140.0
            }
        );
        let origin = absolute_origin(&style, cb(), (99.0, 99.0), (136.0, 66.0), Direction::Ltr);
        assert_eq!(origin.border_origin(), (42.0, 42.0));
        let explicit = computed(
            "position:absolute;left:10%;right:20%;width:50%;padding:5%;box-sizing:border-box",
        );
        assert_eq!(
            absolute_size(&explicit, cb(), 1.0).content_width,
            Some(80.0)
        );
    }

    #[test]
    fn absolute_right_bottom_auto_and_constraints_have_finite_geometry() {
        let style = computed(
            "position:absolute;right:10px;bottom:5px;width:50px;height:20px;margin:2px 3px",
        );
        let origin = absolute_origin(&style, cb(), (0.0, 0.0), (50.0, 20.0), Direction::Ltr);
        assert_eq!(origin.border_origin(), (157.0, 103.0));
        let auto = computed("position:absolute;left:10px;right:10px;width:50px;margin:0 auto");
        assert_eq!(
            absolute_origin(&auto, cb(), (0.0, 30.0), (50.0, 20.0), Direction::Ltr).border_origin(),
            (95.0, 30.0)
        );
        let constrained = computed(
            "position:absolute;left:10px;right:20px;min-width:100px;max-width:80px;min-height:30px;max-height:40px",
        );
        assert_eq!(
            absolute_size(&constrained, cb(), 400.0).content_width,
            Some(100.0)
        );
        let indefinite = ContainingBlock {
            height: None,
            ..cb()
        };
        let style = computed("position:absolute;top:50%;bottom:10%;height:50%;width:50%");
        assert_eq!(
            absolute_size(&style, indefinite, 100.0).content_height,
            None
        );
        assert_eq!(
            absolute_origin(
                &style,
                indefinite,
                (70.0, 80.0),
                (100.0, 10.0),
                Direction::Ltr
            )
            .border_origin(),
            (70.0, 80.0)
        );
    }

    #[test]
    fn absolute_overconstraint_uses_containing_direction_and_static_cursor() {
        let style = computed(
            "position:absolute;left:10px;right:20px;width:40px;top:5px;bottom:10px;height:20px",
        );
        assert_eq!(
            absolute_origin(&style, cb(), (0.0, 0.0), (40.0, 20.0), Direction::Ltr).border_origin(),
            (30.0, 35.0)
        );
        assert_eq!(
            absolute_origin(&style, cb(), (0.0, 0.0), (40.0, 20.0), Direction::Rtl).border_origin(),
            (160.0, 35.0)
        );
        let auto = computed("position:absolute");
        assert_eq!(
            absolute_origin(&auto, cb(), (70.0, 80.0), (40.0, 20.0), Direction::Ltr)
                .border_origin(),
            (70.0, 80.0)
        );
        assert_eq!(absolute_size(&auto, cb(), 500.0).content_width, Some(200.0));
        assert_eq!(absolute_size(&auto, cb(), 40.0).content_width, Some(40.0));
    }

    #[test]
    fn hostile_external_dimensions_and_offsets_are_bounded_without_panics() {
        let style = computed(
            "position:absolute;left:16384%;top:16384%;width:16384%;height:16384%;padding:16384%;margin:-16384%",
        );
        for value in [0.0, 1.0, 1_000_000.0, f32::INFINITY, f32::NAN] {
            let cb = ContainingBlock {
                x: value,
                y: -value,
                width: value,
                height: Some(value),
            };
            let size = absolute_size(&style, cb, value);
            let origin =
                absolute_origin(&style, cb, (value, value), (value, value), Direction::Rtl);
            let (x, y) = origin.border_origin();
            for value in [
                size.content_width.unwrap_or(0.0),
                size.content_height.unwrap_or(0.0),
                size.available_width,
                x,
                y,
            ]
            .into_iter()
            .chain(origin.margins)
            {
                assert!(value.is_finite());
                assert!(value.abs() <= MAX_POSITION_COORD);
            }
        }
    }
}
