use super::*;
use crate::grid::{
    Contribution, GridArea, MinimumContribution, PlacementInput, Track, TrackBreadth,
    minimum_contribution, size_tracks_with_budget,
};
use crate::sizing::{
    AvailableSize, AxisSpace, IntrinsicSizes, content_size, dimension, resolve_content_size,
};
use crate::style::{Alignment, Direction};

struct GridItem {
    node: NodeId,
    area: GridArea,
    width: f32,
    horizontal_inset: f32,
    vertical_inset: f32,
    margins: [f32; 4],
    auto_margins: [bool; 4],
    measured: MeasureResult,
    row_basis: Option<f32>,
    justify: Alignment,
    align: Alignment,
    image: bool,
}

fn implicit_tracks(explicit: &[Track], implicit: &[Track], count: usize) -> Vec<Track> {
    (0..count)
        .map(|index| {
            explicit.get(index).copied().unwrap_or_else(|| {
                if implicit.is_empty() {
                    Track::auto()
                } else {
                    implicit[(index - explicit.len()) % implicit.len()]
                }
            })
        })
        .collect()
}

fn percentage_tracks(mut tracks: Vec<Track>, basis: Option<f32>) -> Vec<Track> {
    // A used min/max constraint supplies alignment/fractional space without
    // making an automatic axis definite for percentage tracks. Normalize those
    // components before passing the separate used size to the numeric solver.
    for track in &mut tracks {
        for breadth in [&mut track.min, &mut track.max] {
            if let TrackBreadth::Length(value) = *breadth
                && value.resolve_indefinite(None).is_none()
            {
                *breadth = value
                    .resolve_indefinite(basis)
                    .map_or(TrackBreadth::Auto, |value| {
                        TrackBreadth::Length(Length::Px(dimension(value)))
                    });
            }
        }
    }
    tracks
}

fn area_size(sizes: &[f32], start: usize, span: usize, gap: f32) -> f32 {
    dimension(sizes[start..start + span].iter().sum::<f32>() + gap * span.saturating_sub(1) as f32)
}

fn effective(value: Alignment, parent: Alignment) -> Alignment {
    if value == Alignment::Auto {
        parent
    } else {
        value
    }
}

fn stretch(value: Alignment) -> bool {
    matches!(value, Alignment::Stretch | Alignment::Auto)
}

fn track_offsets(sizes: &[f32], available: f32, gap: f32, alignment: Alignment) -> (Vec<f32>, f32) {
    let extent = sizes.iter().sum::<f32>() + gap * sizes.len().saturating_sub(1) as f32;
    let free = available - extent;
    let positive = free.max(0.0);
    let count = sizes.len();
    let (mut position, extra_gap) = match alignment {
        Alignment::End | Alignment::FlexEnd => (free, 0.0),
        Alignment::Center => (free / 2.0, 0.0),
        Alignment::SpaceBetween if count > 1 => (0.0, positive / (count - 1) as f32),
        Alignment::SpaceAround if count > 0 => {
            (positive / (count * 2) as f32, positive / count as f32)
        }
        Alignment::SpaceEvenly if count > 0 => {
            (positive / (count + 1) as f32, positive / (count + 1) as f32)
        }
        _ => (0.0, 0.0),
    };
    let mut offsets = Vec::with_capacity(count);
    for size in sizes {
        offsets.push(position.clamp(-MAX_COORD, MAX_COORD));
        position += size + gap + extra_gap;
    }
    (offsets, gap + extra_gap)
}

fn item_offset(
    available: f32,
    border: f32,
    margins: [f32; 2],
    auto_margins: [bool; 2],
    alignment: Alignment,
    reverse: bool,
) -> f32 {
    let [margin_start, margin_end] = margins;
    let [auto_start, auto_end] = auto_margins;
    let free = available - border - margin_start - margin_end;
    let count = usize::from(auto_start) + usize::from(auto_end);
    if count > 0 && free > 0.0 {
        return margin_start + if auto_start { free / count as f32 } else { 0.0 };
    }
    let offset = match alignment {
        Alignment::End | Alignment::FlexEnd => {
            if reverse {
                0.0
            } else {
                free
            }
        }
        Alignment::Center => free / 2.0,
        _ => {
            if reverse {
                free
            } else {
                0.0
            }
        }
    };
    margin_start + offset
}

fn specified_axis(
    style: &ComputedStyle,
    intrinsic: IntrinsicSizes,
    value: Option<Length>,
    available: f32,
    inset: f32,
    horizontal: bool,
) -> Option<f32> {
    value.and_then(|value| {
        if horizontal {
            resolve_content_size(
                intrinsic,
                value,
                AvailableSize::Definite(available),
                inset,
                style.box_sizing,
            )
        } else {
            value
                .resolve(available)
                .map(|value| content_size(value, inset, style.box_sizing))
        }
    })
}

fn constrained_axis(
    style: &ComputedStyle,
    intrinsic: IntrinsicSizes,
    value: f32,
    available: f32,
    inset: f32,
    horizontal: bool,
) -> f32 {
    let (min, max) = if horizontal {
        (style.min_width, style.max_width)
    } else {
        (style.min_height, style.max_height)
    };
    crate::sizing::constrain(
        value,
        specified_axis(style, intrinsic, min, available, inset, horizontal),
        specified_axis(style, intrinsic, max, available, inset, horizontal),
    )
}

impl Builder<'_> {
    fn grid_charge(&mut self, work: usize) -> bool {
        let remaining = self.layout_work.get();
        if work > remaining {
            self.layout_work.set(0);
            self.truncated = true;
            false
        } else {
            self.layout_work.set(remaining - work);
            true
        }
    }

    pub(super) fn grid_children(
        &mut self,
        id: NodeId,
        x: f32,
        y: f32,
        width: f32,
        height_space: AxisSpace,
        depth: usize,
    ) -> f32 {
        let height = height_space.available.definite();
        let percentage_height = height_space.percentage_basis;
        if self.layout_work.get() == 0 {
            self.truncated = true;
            return 0.0;
        }
        let nodes = self.formatting_items(id);
        if !self.grid_charge(nodes.len()) {
            return 0.0;
        }
        let preceding_baseline = self.first_flow_baseline;
        let style = &self.styles[id];
        let placement_input: Vec<_> = nodes
            .iter()
            .map(|&node| PlacementInput {
                row: self.styles[node].grid_row,
                column: self.styles[node].grid_column,
            })
            .collect();
        let placement = crate::grid::place_items_with_budget(
            &placement_input,
            style.grid_template_rows.len(),
            style.grid_template_columns.len(),
            style.grid_auto_flow,
            self.layout_work.get(),
        );
        self.truncated |= placement.truncated;
        if !self.grid_charge(placement.work) {
            return 0.0;
        }
        let columns = implicit_tracks(
            &style.grid_template_columns,
            &style.grid_auto_columns,
            placement.columns,
        );
        let rows = percentage_tracks(
            implicit_tracks(
                &style.grid_template_rows,
                &style.grid_auto_rows,
                placement.rows,
            ),
            percentage_height,
        );
        let column_gap = dimension(style.column_gap.resolve(width).unwrap_or(0.0));
        let row_gap = dimension(
            style
                .row_gap
                .resolve_indefinite(percentage_height)
                .unwrap_or(0.0),
        );
        let mut column_items = Vec::new();
        for (index, &node) in nodes.iter().enumerate() {
            let Some(area) = placement.areas.get(index).copied().flatten() else {
                continue;
            };
            if !self.grid_charge(3 * area.column_span + 1) {
                return 0.0;
            }
            let item_style = &self.styles[node];
            let intrinsic = self.intrinsic.content(node);
            let contribution = self.intrinsic.contribution(node);
            let indefinite = |value: Length| value.resolve_indefinite(None).unwrap_or(0.0).max(0.0);
            let inset = indefinite(item_style.padding.left)
                + indefinite(item_style.padding.right)
                + if item_style.border_style == BorderStyle::None {
                    0.0
                } else {
                    indefinite(item_style.border_width.left)
                        + indefinite(item_style.border_width.right)
                };
            let margins = item_style
                .margin
                .left
                .resolve_indefinite(None)
                .unwrap_or(0.0)
                + item_style
                    .margin
                    .right
                    .resolve_indefinite(None)
                    .unwrap_or(0.0);
            let spanned = &columns[area.column..area.column + area.column_span];
            let resolve = |value: Option<Length>| {
                value.and_then(|value| {
                    resolve_content_size(
                        intrinsic,
                        value,
                        AvailableSize::Indefinite,
                        inset,
                        item_style.box_sizing,
                    )
                })
            };
            let minimum = minimum_contribution(
                spanned,
                AvailableSize::Definite(width),
                column_gap,
                MinimumContribution {
                    content_min: intrinsic.min_content,
                    preferred: resolve(item_style.width),
                    min: resolve(item_style.min_width),
                    max: resolve(item_style.max_width),
                    inset,
                    margins,
                    preferred_outer: contribution.min_content,
                    overflow_hidden: item_style.overflow_hidden,
                },
            );
            column_items.push(Contribution {
                start: area.column,
                span: area.column_span,
                min_content: contribution.min_content,
                max_content: contribution.max_content,
                minimum: dimension(minimum),
            });
        }
        let column_sizes = size_tracks_with_budget(
            &columns,
            AvailableSize::Definite(width),
            column_gap,
            &column_items,
            stretch(style.justify_content),
            self.layout_work.get(),
        );
        self.truncated |= column_sizes.truncated;
        if !self.grid_charge(column_sizes.work) {
            return 0.0;
        }
        let (column_offsets, used_column_gap) = track_offsets(
            &column_sizes.sizes,
            width,
            column_gap,
            style.justify_content,
        );
        // An initial definite-axis solution gives percentage heights a grid-area
        // basis. Indefinite intrinsically sized rows keep percentages unresolved.
        let initial_rows = size_tracks_with_budget(
            &rows,
            height_space.available,
            row_gap,
            &[],
            stretch(style.align_content),
            self.layout_work.get(),
        );
        self.truncated |= initial_rows.truncated;
        if !self.grid_charge(initial_rows.work) {
            return 0.0;
        }
        let mut items = Vec::new();
        for (index, &node) in nodes.iter().enumerate() {
            let Some(area) = placement.areas.get(index).copied().flatten() else {
                continue;
            };
            // Reserve the later row-minimum helper's bounded track scans before
            // dry child layout can recurse and consume the shared allowance.
            if !self.grid_charge(3 * area.row_span + 1) {
                return 0.0;
            }
            let item_style = &self.styles[node];
            let intrinsic = self.intrinsic.content(node);
            let area_width = area_size(
                &column_sizes.sizes,
                area.column,
                area.column_span,
                used_column_gap,
            );
            let (padding, border) = inset(item_style, area_width);
            let horizontal_inset = padding[1] + padding[3] + border[1] + border[3];
            let vertical_inset = padding[0] + padding[2] + border[0] + border[2];
            let margins = edge_values(item_style.margin, area_width);
            let auto_margins = [
                item_style.margin.top,
                item_style.margin.right,
                item_style.margin.bottom,
                item_style.margin.left,
            ]
            .map(|value| matches!(value, Length::Auto));
            let justify = effective(item_style.justify_self, style.justify_items);
            let align = effective(item_style.align_self, style.align_items);
            let image = self
                .document
                .element(node)
                .is_some_and(|element| element.tag == "img");
            let preferred = specified_axis(
                item_style,
                intrinsic,
                item_style.width,
                area_width,
                horizontal_inset,
                true,
            );
            let stretch_width = stretch(justify)
                && !auto_margins[1]
                && !auto_margins[3]
                && (!image || item_style.justify_self == Alignment::Stretch);
            let width = preferred.unwrap_or_else(|| {
                let available = (area_width - margins[1] - margins[3] - horizontal_inset).max(0.0);
                if stretch_width {
                    available
                } else {
                    available
                        .max(intrinsic.min_content)
                        .min(intrinsic.max_content)
                }
            });
            let width = constrained_axis(
                item_style,
                intrinsic,
                width,
                area_width,
                horizontal_inset,
                true,
            );
            let row_basis = if percentage_height.is_some() || rows[area.row..area.row + area.row_span].iter().all(|track| track.min == track.max && matches!(track.max, TrackBreadth::Length(value) if value.resolve_indefinite(None).is_some())) {
                    Some(area_size(&initial_rows.sizes, area.row, area.row_span, row_gap))
                } else { None };
            let measured = self.measure_item(
                node,
                area_width,
                row_basis,
                SizeOverride {
                    width: Some(width),
                    height: None,
                    suppress_margins: true,
                    indefinite_height: false,
                },
                depth,
            );
            items.push(GridItem {
                node,
                area,
                width,
                horizontal_inset,
                vertical_inset,
                margins,
                auto_margins,
                measured,
                row_basis,
                justify,
                align,
                image,
            });
        }
        let mut row_baselines = vec![0.0f32; placement.rows];
        for item in &items {
            if item.area.row_span == 1
                && item.align == Alignment::Baseline
                && !item.auto_margins[0]
                && !item.auto_margins[2]
                && let Some(baseline) = item.measured.baseline
            {
                row_baselines[item.area.row] =
                    row_baselines[item.area.row].max(item.margins[0] + baseline);
            }
        }
        let row_items: Vec<_> = items
            .iter()
            .map(|item| {
                let style = &self.styles[item.node];
                let shim = if item.area.row_span == 1
                    && item.align == Alignment::Baseline
                    && !item.auto_margins[0]
                    && !item.auto_margins[2]
                {
                    item.measured.baseline.map_or(0.0, |baseline| {
                        (row_baselines[item.area.row] - item.margins[0] - baseline).max(0.0)
                    })
                } else {
                    0.0
                };
                let size =
                    dimension(item.measured.height + item.margins[0] + item.margins[2] + shim);
                let resolve = |value: Option<Length>| {
                    value.and_then(|value| {
                        value
                            .resolve_indefinite(item.row_basis)
                            .map(|value| content_size(value, item.vertical_inset, style.box_sizing))
                    })
                };
                let minimum = minimum_contribution(
                    &rows[item.area.row..item.area.row + item.area.row_span],
                    height_space.available,
                    row_gap,
                    MinimumContribution {
                        content_min: (item.measured.height - item.vertical_inset).max(0.0),
                        preferred: resolve(style.height),
                        min: resolve(style.min_height),
                        max: resolve(style.max_height),
                        inset: item.vertical_inset,
                        margins: item.margins[0] + item.margins[2],
                        preferred_outer: size - shim,
                        overflow_hidden: style.overflow_hidden,
                    },
                );
                Contribution {
                    start: item.area.row,
                    span: item.area.row_span,
                    min_content: size,
                    max_content: size,
                    minimum: dimension(minimum + shim),
                }
            })
            .collect();
        let row_sizes = size_tracks_with_budget(
            &rows,
            height_space.available,
            row_gap,
            &row_items,
            stretch(style.align_content),
            self.layout_work.get(),
        );
        self.truncated |= row_sizes.truncated;
        if !self.grid_charge(row_sizes.work) {
            return row_sizes.extent;
        }
        let container_height = height.unwrap_or(row_sizes.extent);
        let (row_offsets, used_row_gap) = track_offsets(
            &row_sizes.sizes,
            container_height,
            row_gap,
            style.align_content,
        );
        // Explicit placement may leave leading rows empty or make source order
        // differ from row-major grid order. A sharing group's baseline takes
        // priority over the earliest column in the first occupied row.
        let first_row = items.iter().map(|item| item.area.row).min();
        let baseline_item = items
            .iter()
            .enumerate()
            .filter(|(_, item)| Some(item.area.row) == first_row)
            .min_by_key(|(index, item)| {
                (
                    !(item.area.row_span == 1
                        && item.align == Alignment::Baseline
                        && !item.auto_margins[0]
                        && !item.auto_margins[2]),
                    item.area.column,
                    *index,
                )
            })
            .map(|(index, _)| index);
        let mut first_baseline = None;
        for (index, item) in items.iter().enumerate() {
            if self.layout_work.get() == 0 {
                self.truncated = true;
                break;
            }
            let item_style = &self.styles[item.node];
            let intrinsic = self.intrinsic.content(item.node);
            let area_width = area_size(
                &column_sizes.sizes,
                item.area.column,
                item.area.column_span,
                used_column_gap,
            );
            let area_height = area_size(
                &row_sizes.sizes,
                item.area.row,
                item.area.row_span,
                used_row_gap,
            );
            let border_width = item.width + item.horizontal_inset;
            let preferred = specified_axis(
                item_style,
                intrinsic,
                item_style.height,
                area_height,
                item.vertical_inset,
                false,
            );
            let stretch_height = stretch(item.align)
                && !item.auto_margins[0]
                && !item.auto_margins[2]
                && (!item.image || item_style.align_self == Alignment::Stretch);
            let content_height = preferred.unwrap_or_else(|| {
                if stretch_height {
                    (area_height - item.margins[0] - item.margins[2] - item.vertical_inset).max(0.0)
                } else {
                    (item.measured.height - item.vertical_inset).max(0.0)
                }
            });
            let content_height = constrained_axis(
                item_style,
                intrinsic,
                content_height,
                area_height,
                item.vertical_inset,
                false,
            );
            let border_height = content_height + item.vertical_inset;
            let mut area_x = column_offsets[item.area.column];
            let rtl = style.direction == Direction::Rtl;
            if rtl {
                area_x = width - area_x - area_width;
            }
            let offset_x = item_offset(
                area_width,
                border_width,
                [item.margins[3], item.margins[1]],
                [item.auto_margins[3], item.auto_margins[1]],
                item.justify,
                rtl,
            );
            let offset_y = if item.area.row_span == 1
                && item.align == Alignment::Baseline
                && !item.auto_margins[0]
                && !item.auto_margins[2]
            {
                item.measured.baseline.map_or(item.margins[0], |baseline| {
                    row_baselines[item.area.row] - baseline
                })
            } else {
                item_offset(
                    area_height,
                    border_height,
                    [item.margins[0], item.margins[2]],
                    [item.auto_margins[0], item.auto_margins[2]],
                    item.align,
                    false,
                )
            };
            let item_y = y + row_offsets[item.area.row] + offset_y;
            if Some(index) == baseline_item {
                first_baseline = Some(item_y + item.measured.baseline.unwrap_or(border_height));
            }
            self.lay_out_item(
                item.node,
                (x + area_x + offset_x, item_y),
                area_width,
                Some(area_height),
                SizeOverride {
                    width: Some(item.width),
                    height: Some(content_height),
                    suppress_margins: true,
                    // The area resolves an item's own percentage height. An
                    // unstretched natural item still has an automatic height,
                    // so forcing its measured result must not give percentage
                    // descendants a definite containing-height basis.
                    indefinite_height: preferred.is_none() && !stretch_height,
                },
                depth,
            );
        }
        self.last_flow_baseline = first_baseline.or(self.last_flow_baseline);
        if preceding_baseline.is_none() {
            self.first_flow_baseline = first_baseline.or(self.first_flow_baseline);
        }
        row_sizes.extent
    }
}
