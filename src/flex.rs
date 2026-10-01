//! Bounded numerical Flexbox layout for horizontal writing.
//!
//! DOM traversal, intrinsic measurement and child layout belong to the shared
//! layout/sizing layer. This module consumes content-box bases and constraints;
//! it never emits paint or shapes text. Main sizing precedes width-dependent
//! cross measurement. Final positions are physical low-edge border-box offsets
//! (x for rows/y for columns on the main axis), independent of source order.
//!
//! The freeze loop follows CSS Flexbox 1 section 9.7, including scaled shrink
//! factors and partial-fill factors below one. At most 4,096 items are accepted;
//! each freeze pass freezes at least one item and at most n+1 passes are made.

use std::ops::Range;

use crate::sizing::AvailableSize;
use crate::style::Alignment;

pub const MAX_FLEX_ITEMS: usize = 4096;
pub const MAX_FLEX_COORD: f32 = 1_000_000.0;
/// Numeric item/line visits per two-phase plan, independent of DOM work.
pub const MAX_FLEX_WORK: usize = 4_000_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FlexDirection {
    #[default]
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl FlexDirection {
    pub fn is_row(self) -> bool {
        matches!(self, Self::Row | Self::RowReverse)
    }

    pub fn is_reverse(self) -> bool {
        matches!(self, Self::RowReverse | Self::ColumnReverse)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FlexWrap {
    #[default]
    NoWrap,
    Wrap,
    WrapReverse,
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Inner content-box sizes; an indefinite main axis does not flex.
    pub main_size: AvailableSize,
    /// Optional independent line-collection constraint. None uses main_size.
    /// Some(Indefinite) prevents a min-constrained auto height from wrapping;
    /// Some(Definite(maximum)) permits max-height wrapping before used sizing.
    pub collection_size: Option<AvailableSize>,
    pub cross_size: AvailableSize,
    pub direction: FlexDirection,
    pub wrap: FlexWrap,
    pub rtl: bool,
    pub main_gap: f32,
    pub cross_gap: f32,
    pub justify_content: Alignment,
    pub align_items: Alignment,
    pub align_content: Alignment,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            main_size: AvailableSize::Indefinite,
            collection_size: None,
            cross_size: AvailableSize::Indefinite,
            direction: FlexDirection::Row,
            wrap: FlexWrap::NoWrap,
            rtl: false,
            main_gap: 0.0,
            cross_gap: 0.0,
            justify_content: Alignment::FlexStart,
            align_items: Alignment::Stretch,
            align_content: Alignment::Stretch,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Item {
    pub order: i32,
    /// Unclamped inner base; may be negative for zero border-box bases.
    pub base_size: f32,
    pub min_size: f32,
    pub max_size: Option<f32>,
    pub main_inset: f32,
    pub cross_inset: f32,
    /// Physical [low-edge, high-edge] margins; None is an auto margin.
    pub main_margin: [Option<f32>; 2],
    pub cross_margin: [Option<f32>; 2],
    pub grow: f32,
    pub shrink: f32,
    pub cross_auto: bool,
    pub min_cross: f32,
    pub max_cross: Option<f32>,
    pub align_self: Alignment,
}

impl Default for Item {
    fn default() -> Self {
        Self {
            order: 0,
            base_size: 0.0,
            min_size: 0.0,
            max_size: None,
            main_inset: 0.0,
            cross_inset: 0.0,
            main_margin: [Some(0.0); 2],
            cross_margin: [Some(0.0); 2],
            grow: 0.0,
            shrink: 1.0,
            cross_auto: true,
            min_cross: 0.0,
            max_cross: None,
            align_self: Alignment::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CrossMeasurement {
    /// Natural cross-axis border size after layout at the resolved main size.
    pub border_size: f32,
    /// Existing first baseline from the physical low border edge. Only row
    /// baseline alignment is supported; missing baselines synthesize the bottom.
    pub baseline: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct ItemPlacement {
    pub source_index: usize,
    pub line: usize,
    pub main_size: f32,
    pub main_position: f32,
    pub cross_size: f32,
    pub cross_position: f32,
    pub main_margin: [f32; 2],
    pub cross_margin: [f32; 2],
}

#[derive(Debug, Clone, Default)]
pub struct FlexLine {
    /// Range in Plan.order, never a range in the logical DOM item array.
    pub items: Range<usize>,
    pub cross_size: f32,
    pub cross_position: f32,
    pub baseline: f32,
    pub freeze_iterations: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Plan {
    /// Stable order-modified document order. Reverse directions do not reverse
    /// this array; geometry alone changes direction. Use it for paint ordering.
    pub order: Vec<usize>,
    /// Indexed by original logical source index; source items are never mutated.
    pub items: Vec<ItemPlacement>,
    pub lines: Vec<FlexLine>,
    pub main_size: f32,
    pub cross_size: f32,
    pub truncated: bool,
    /// Actual numeric item and line visits; excludes allocation and sorting.
    /// The shared layout layer additionally charges this to its global budget.
    pub work: usize,
}

fn coordinate(value: f64) -> f32 {
    if value.is_finite() {
        value.clamp(-f64::from(MAX_FLEX_COORD), f64::from(MAX_FLEX_COORD)) as f32
    } else {
        0.0
    }
}

fn output_coordinate(value: f64, truncated: &mut bool) -> f32 {
    if !value.is_finite() || value.abs() > f64::from(MAX_FLEX_COORD) {
        *truncated = true;
    }
    coordinate(value)
}

fn size(value: f32) -> f64 {
    f64::from(coordinate(f64::from(value)).max(0.0))
}

fn margin(value: Option<f32>) -> f64 {
    f64::from(coordinate(f64::from(value.unwrap_or(0.0))))
}

fn base(item: &Item) -> f64 {
    f64::from(coordinate(f64::from(item.base_size)))
}

fn constrained(value: f64, minimum: f32, maximum: Option<f32>) -> f64 {
    let minimum = size(minimum);
    value
        .min(maximum.map(size).unwrap_or(f64::from(MAX_FLEX_COORD)))
        .max(minimum)
        .max(0.0)
}

fn main_outer(item: &Item, content: f64) -> f64 {
    content + size(item.main_inset) + margin(item.main_margin[0]) + margin(item.main_margin[1])
}

fn main_reverse(config: &Config) -> bool {
    config.direction.is_reverse() ^ (config.direction.is_row() && config.rtl)
}

fn cross_reverse(config: &Config) -> bool {
    (config.wrap == FlexWrap::WrapReverse) ^ (!config.direction.is_row() && config.rtl)
}

fn factor(item: &Item, grow: bool) -> f64 {
    size(if grow { item.grow } else { item.shrink })
}

fn alignment(item: &Item, config: &Config) -> Alignment {
    if item.align_self == Alignment::Auto {
        config.align_items
    } else {
        item.align_self
    }
}

/// Returns flow-relative initial offset and extra space between neighbors.
/// `logical_reverse` describes reversal relative to writing-mode start, not
/// physical RTL: start/end remain independent from flex-start/flex-end.
fn distribute(alignment: Alignment, free: f64, count: usize, logical_reverse: bool) -> (f64, f64) {
    match alignment {
        Alignment::Start if logical_reverse => (free, 0.0),
        Alignment::End if !logical_reverse => (free, 0.0),
        Alignment::FlexEnd => (free, 0.0),
        Alignment::Center => (free / 2.0, 0.0),
        Alignment::SpaceBetween if free >= 0.0 && count > 1 => (0.0, free / (count - 1) as f64),
        Alignment::SpaceAround if free >= 0.0 && count > 0 => {
            let spacing = free / count as f64;
            (spacing / 2.0, spacing)
        }
        Alignment::SpaceEvenly if free >= 0.0 && count > 0 => {
            let spacing = free / (count + 1) as f64;
            (spacing, spacing)
        }
        // Distributed alignment's overflow fallback is safe start. Unsupported
        // main-axis baseline/stretch/auto also consistently use flex-start.
        _ => (0.0, 0.0),
    }
}

fn flex_line(
    indices: &[usize],
    items: &[Item],
    available: f64,
    work: &mut usize,
    freeze_budget: usize,
) -> (Vec<f64>, usize, bool) {
    let hypothetical: Vec<_> = indices
        .iter()
        .map(|&i| constrained(base(&items[i]), items[i].min_size, items[i].max_size))
        .collect();
    *work += indices.len();
    let grow = indices
        .iter()
        .zip(&hypothetical)
        .map(|(&i, &value)| main_outer(&items[i], value))
        .sum::<f64>()
        < available;
    *work += indices.len();
    let mut targets: Vec<_> = indices.iter().map(|&i| base(&items[i])).collect();
    *work += indices.len();
    let mut frozen = vec![false; indices.len()];
    let mut unfrozen = indices.len();
    for (position, &i) in indices.iter().enumerate() {
        let item = &items[i];
        if factor(item, grow) == 0.0
            || (grow && base(item) > hypothetical[position])
            || (!grow && base(item) < hypothetical[position])
        {
            targets[position] = hypothetical[position];
            frozen[position] = true;
            unfrozen -= 1;
        }
    }
    *work += indices.len();
    let remaining = |targets: &[f64], frozen: &[bool]| {
        available
            - indices
                .iter()
                .enumerate()
                .map(|(position, &i)| {
                    main_outer(
                        &items[i],
                        if frozen[position] {
                            targets[position]
                        } else {
                            base(&items[i])
                        },
                    )
                })
                .sum::<f64>()
    };
    let initial_free = remaining(&targets, &frozen);
    *work += indices.len();
    let mut iterations = 0;
    let mut violations = vec![0.0; indices.len()];
    let mut truncated = false;
    while unfrozen > 0 && iterations <= indices.len() {
        // Reserve all three scans before starting an iteration. When hostile
        // constraints require too many freeze passes, retain the last complete
        // pass, clamp every target, and report truncation. Final placement and
        // cross sizing still run, so an exhausted plan remains inspectable.
        if work.saturating_add(indices.len() * 3) > freeze_budget {
            for (position, &i) in indices.iter().enumerate() {
                targets[position] =
                    constrained(targets[position], items[i].min_size, items[i].max_size);
            }
            *work += indices.len();
            truncated = true;
            break;
        }
        iterations += 1;
        let mut free = available;
        let mut factors = 0.0;
        let mut scaled = 0.0;
        for (position, &i) in indices.iter().enumerate() {
            let item = &items[i];
            free -= main_outer(
                item,
                if frozen[position] {
                    targets[position]
                } else {
                    base(item)
                },
            );
            if !frozen[position] {
                factors += factor(item, grow);
                scaled += factor(item, grow) * base(item).max(0.0);
            }
        }
        *work += indices.len();
        if factors < 1.0 && (initial_free * factors).abs() < free.abs() {
            free = initial_free * factors;
        }
        let mut total_violation = 0.0;
        for (position, &i) in indices.iter().enumerate() {
            violations[position] = 0.0;
            if frozen[position] {
                continue;
            }
            let item = &items[i];
            let fraction = if grow && factors > 0.0 {
                factor(item, true) / factors
            } else if !grow && scaled > 0.0 {
                factor(item, false) * base(item).max(0.0) / scaled
            } else {
                0.0
            };
            let proposed = if grow {
                base(item) + free * fraction
            } else {
                base(item) - free.abs() * fraction
            };
            let target = constrained(proposed, item.min_size, item.max_size);
            violations[position] = target - proposed;
            total_violation += violations[position];
            targets[position] = target;
        }
        *work += indices.len();
        let before = unfrozen;
        for position in 0..indices.len() {
            if !frozen[position]
                && (total_violation.abs() < 0.000_001
                    || (total_violation > 0.0 && violations[position] > 0.0)
                    || (total_violation < 0.0 && violations[position] < 0.0))
            {
                frozen[position] = true;
                unfrozen -= 1;
            }
        }
        *work += indices.len();
        // Floating-point cancellation must never prevent progress. This is only
        // a numerical fallback; exact min/max cases freeze by violation sign.
        if unfrozen == before {
            unfrozen = 0;
        }
    }
    (targets, iterations, truncated)
}

/// Collect lines and resolve actual flex lengths. Percentage resolution,
/// automatic minima and basis selection are completed by shared sizing before
/// this call. Main-axis auto margins and justification are resolved here.
pub fn resolve_main(config: &Config, items: &[Item]) -> Plan {
    resolve_main_with_budget(config, items, MAX_FLEX_WORK)
}

/// The shared document budget can be stricter than the local plan budget.
/// Reserve sixteen linear visits per accepted item so final placement and
/// one subsequent cross pass remain within the supplied complete-plan limit.
/// A small budget selects a deterministic source prefix before order sorting.
pub fn resolve_main_with_budget(config: &Config, items: &[Item], budget: usize) -> Plan {
    let budget = budget.min(MAX_FLEX_WORK);
    let count = items.len().min(MAX_FLEX_ITEMS).min(budget / 16);
    let truncated = count < items.len();
    let items = &items[..count];
    let mut plan = Plan {
        order: (0..count).collect(),
        items: (0..count)
            .map(|source_index| ItemPlacement {
                source_index,
                ..ItemPlacement::default()
            })
            .collect(),
        truncated,
        ..Plan::default()
    };
    // The input prefix is selected before sorting, so truncation does not depend
    // on an attacker-controlled order value or disappear from source order.
    plan.order.sort_by_key(|&i| (items[i].order, i));
    let definite = config.main_size.definite().map(size);
    let collection_limit = config
        .collection_size
        .unwrap_or(config.main_size)
        .definite()
        .map(size);
    let gap = size(config.main_gap);
    let mut line_start = 0;
    let mut used = 0.0;
    for position in 0..count {
        let item = &items[plan.order[position]];
        let hypothetical = constrained(base(item), item.min_size, item.max_size);
        let outer = main_outer(item, hypothetical);
        if position > line_start
            && config.wrap != FlexWrap::NoWrap
            && collection_limit.is_some_and(|limit| used + gap + outer > limit)
        {
            plan.lines.push(FlexLine {
                items: line_start..position,
                ..FlexLine::default()
            });
            line_start = position;
            used = 0.0;
        }
        used += if position > line_start { gap } else { 0.0 } + outer;
    }
    plan.work += count;
    if count > 0 {
        plan.lines.push(FlexLine {
            items: line_start..count,
            ..FlexLine::default()
        });
    }
    for (line_index, line) in plan.lines.iter_mut().enumerate() {
        let indices = &plan.order[line.items.clone()];
        let gaps = gap * indices.len().saturating_sub(1) as f64;
        let (targets, iterations, exhausted) = if let Some(available) = definite {
            flex_line(
                indices,
                items,
                available - gaps,
                &mut plan.work,
                // At most sixteen linear visits remain per item across main/cross
                // sizing, including fallback clamps and one-item lines.
                budget - count * 16,
            )
        } else {
            plan.work += indices.len();
            (
                indices
                    .iter()
                    .map(|&i| constrained(base(&items[i]), items[i].min_size, items[i].max_size))
                    .collect(),
                0,
                false,
            )
        };
        plan.truncated |= exhausted;
        line.freeze_iterations = iterations;
        let occupied = indices
            .iter()
            .zip(&targets)
            .map(|(&i, &value)| main_outer(&items[i], value))
            .sum::<f64>()
            + gaps;
        plan.work += indices.len();
        let container = definite.unwrap_or(occupied.max(0.0));
        plan.main_size = plan
            .main_size
            .max(output_coordinate(container, &mut plan.truncated).max(0.0));
        let free = container - occupied;
        let autos = indices
            .iter()
            .map(|&i| {
                items[i]
                    .main_margin
                    .iter()
                    .filter(|value| value.is_none())
                    .count()
            })
            .sum::<usize>();
        plan.work += indices.len();
        let auto_margin = if autos > 0 && free > 0.0 {
            free / autos as f64
        } else {
            0.0
        };
        let (mut cursor, extra_gap) = distribute(
            config.justify_content,
            if autos > 0 && free > 0.0 { 0.0 } else { free },
            indices.len(),
            config.direction.is_reverse(),
        );
        let reverse = main_reverse(config);
        for (&i, &target) in indices.iter().zip(&targets) {
            let item = &items[i];
            let margins = item
                .main_margin
                .map(|value| value.map_or(auto_margin, |v| margin(Some(v))));
            let before = usize::from(reverse);
            let after = 1 - before;
            cursor += margins[before];
            let border = target + size(item.main_inset);
            let position = if reverse {
                container - cursor - border
            } else {
                cursor
            };
            plan.items[i].line = line_index;
            plan.items[i].main_size = output_coordinate(target, &mut plan.truncated).max(0.0);
            plan.items[i].main_position = output_coordinate(position, &mut plan.truncated);
            plan.items[i].main_margin = margins.map(coordinate);
            cursor += border + margins[after] + gap + extra_gap;
        }
        plan.work += indices.len();
    }
    if let Some(available) = definite {
        plan.main_size = output_coordinate(available, &mut plan.truncated).max(0.0);
    }
    plan
}

/// Resolve line cross sizes, baseline groups, stretching, cross auto margins
/// and item/content alignment. Measurements are in logical source order and
/// must come from layout at each Plan.items[i].main_size. The returned content
/// cross size is the forced final child size, including supported stretching.
pub fn resolve_cross(
    config: &Config,
    items: &[Item],
    measurements: &[CrossMeasurement],
    plan: &mut Plan,
) {
    let count = plan.items.len().min(items.len()).min(measurements.len());
    if count < plan.items.len() {
        plan.truncated = true;
        return;
    }
    let cross_gap = size(config.cross_gap);
    for line in &mut plan.lines {
        let mut largest: f64 = 0.0;
        let mut ascent: f64 = 0.0;
        let mut descent: f64 = 0.0;
        for &i in &plan.order[line.items.clone()] {
            let item = &items[i];
            let measurement = measurements[i];
            let border = constrained(
                size(measurement.border_size) - size(item.cross_inset),
                item.min_cross,
                item.max_cross,
            ) + size(item.cross_inset);
            let low = margin(item.cross_margin[0]);
            let high = margin(item.cross_margin[1]);
            if config.direction.is_row()
                && alignment(item, config) == Alignment::Baseline
                && item.cross_margin.iter().all(Option::is_some)
            {
                let baseline = measurement.baseline.map(size).unwrap_or(border);
                ascent = ascent.max(low + baseline);
                descent = descent.max(high + border - baseline);
            } else {
                largest = largest.max(border + low + high);
            }
        }
        plan.work += line.items.len();
        line.cross_size =
            output_coordinate(largest.max(ascent + descent), &mut plan.truncated).max(0.0);
        // Temporarily keep the baseline group's cross-start extent. Reversed
        // row lines anchor their group by descent, including space added by
        // align-content:stretch; final baseline is physical below.
        line.baseline = output_coordinate(
            if config.direction.is_row() && config.wrap == FlexWrap::WrapReverse {
                descent
            } else {
                ascent
            },
            &mut plan.truncated,
        )
        .max(0.0);
    }
    let definite = config.cross_size.definite().map(size);
    if config.wrap == FlexWrap::NoWrap
        && let (Some(available), Some(line)) = (definite, plan.lines.first_mut())
    {
        line.cross_size = output_coordinate(available, &mut plan.truncated).max(0.0);
    }
    let natural = plan
        .lines
        .iter()
        .map(|line| f64::from(line.cross_size))
        .sum::<f64>()
        + cross_gap * plan.lines.len().saturating_sub(1) as f64;
    plan.work += plan.lines.len();
    let container = definite.unwrap_or(natural.max(0.0));
    plan.cross_size = output_coordinate(container, &mut plan.truncated).max(0.0);
    let mut free = container - natural;
    if config.wrap != FlexWrap::NoWrap
        && config.align_content == Alignment::Stretch
        && free > 0.0
        && !plan.lines.is_empty()
    {
        let addition = free / plan.lines.len() as f64;
        for line in &mut plan.lines {
            line.cross_size =
                output_coordinate(f64::from(line.cross_size) + addition, &mut plan.truncated)
                    .max(0.0);
        }
        plan.work += plan.lines.len();
        free = 0.0;
    }
    let (mut line_cursor, extra_gap) = if config.wrap == FlexWrap::NoWrap {
        (0.0, 0.0)
    } else {
        distribute(
            config.align_content,
            free,
            plan.lines.len(),
            config.wrap == FlexWrap::WrapReverse,
        )
    };
    let reverse = cross_reverse(config);
    for line in &mut plan.lines {
        let line_size = f64::from(line.cross_size);
        if config.direction.is_row() && config.wrap == FlexWrap::WrapReverse {
            line.baseline =
                output_coordinate(line_size - f64::from(line.baseline), &mut plan.truncated);
        }
        let physical_line = if reverse {
            container - line_cursor - line_size
        } else {
            line_cursor
        };
        line.cross_position = output_coordinate(physical_line, &mut plan.truncated);
        for &i in &plan.order[line.items.clone()] {
            let item = &items[i];
            let measurement = measurements[i];
            let align = alignment(item, config);
            let has_auto = item.cross_margin.iter().any(Option::is_none);
            let mut margins = item.cross_margin.map(margin);
            let content = if item.cross_auto && align == Alignment::Stretch && !has_auto {
                constrained(
                    line_size - margins[0] - margins[1] - size(item.cross_inset),
                    item.min_cross,
                    item.max_cross,
                )
            } else {
                constrained(
                    size(measurement.border_size) - size(item.cross_inset),
                    item.min_cross,
                    item.max_cross,
                )
            };
            let border = content + size(item.cross_inset);
            let available = line_size - border - margins[0] - margins[1];
            let before = usize::from(reverse);
            let position = if has_auto {
                let autos = item
                    .cross_margin
                    .iter()
                    .filter(|value| value.is_none())
                    .count();
                if available > 0.0 {
                    for (edge, value) in margins.iter_mut().enumerate() {
                        if item.cross_margin[edge].is_none() {
                            *value = available / autos as f64;
                        }
                    }
                } else {
                    // CSS 9.6 overflow: writing-mode start auto becomes zero;
                    // the opposite margin absorbs the overflow.
                    let writing_start = usize::from(!config.direction.is_row() && config.rtl);
                    let writing_end = 1 - writing_start;
                    if item.cross_margin[writing_start].is_none() {
                        margins[writing_start] = 0.0;
                    }
                    margins[writing_end] = line_size - border - margins[writing_start];
                }
                physical_line + margins[0]
            } else if align == Alignment::Baseline && config.direction.is_row() {
                physical_line + f64::from(line.baseline)
                    - measurement.baseline.map(size).unwrap_or(border)
            } else {
                let (offset, _) =
                    distribute(align, available, 1, config.wrap == FlexWrap::WrapReverse);
                if reverse {
                    physical_line + line_size - margins[before] - offset - border
                } else {
                    physical_line + margins[before] + offset
                }
            };
            plan.items[i].cross_size = output_coordinate(content, &mut plan.truncated).max(0.0);
            plan.items[i].cross_position = output_coordinate(position, &mut plan.truncated);
            plan.items[i].cross_margin = margins.map(coordinate);
        }
        plan.work += line.items.len() + 1;
        line_cursor += line_size + cross_gap + extra_gap;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(main: f32) -> Config {
        Config {
            main_size: AvailableSize::Definite(main),
            ..Config::default()
        }
    }

    fn item(base: f32) -> Item {
        Item {
            base_size: base,
            ..Item::default()
        }
    }

    fn close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.002, "{actual} != {expected}");
    }

    fn cross(config: &Config, items: &[Item], sizes: &[f32]) -> Plan {
        let mut plan = resolve_main(config, items);
        let measurements: Vec<_> = sizes
            .iter()
            .map(|&border_size| CrossMeasurement {
                border_size,
                baseline: None,
            })
            .collect();
        resolve_cross(config, items, &measurements, &mut plan);
        plan
    }

    #[test]
    fn unequal_grow_uses_remaining_space_after_bases_and_gaps() {
        let mut config = config(330.0);
        config.main_gap = 30.0;
        let mut a = item(50.0);
        a.grow = 1.0;
        let mut b = item(50.0);
        b.grow = 3.0;
        let plan = resolve_main(&config, &[a, b]);
        close(plan.items[0].main_size, 100.0);
        close(plan.items[1].main_size, 200.0);
        close(plan.items[1].main_position, 130.0);
    }

    #[test]
    fn scaled_shrink_accounts_for_basis_and_shrink_factor() {
        let mut b = item(100.0);
        b.shrink = 2.0;
        let plan = resolve_main(&config(300.0), &[item(200.0), b, item(100.0)]);
        close(plan.items[0].main_size, 160.0);
        close(plan.items[1].main_size, 60.0);
        close(plan.items[2].main_size, 80.0);
    }

    #[test]
    fn grow_maximum_freezes_then_redistributes() {
        let mut a = item(0.0);
        a.grow = 1.0;
        a.max_size = Some(50.0);
        let mut b = item(0.0);
        b.grow = 1.0;
        let plan = resolve_main(&config(300.0), &[a, b]);
        close(plan.items[0].main_size, 50.0);
        close(plan.items[1].main_size, 250.0);
        assert_eq!(plan.lines[0].freeze_iterations, 2);
    }

    #[test]
    fn shrink_minimum_freezes_then_redistributes() {
        let mut a = item(200.0);
        a.min_size = 180.0;
        let plan = resolve_main(&config(300.0), &[a, item(200.0)]);
        close(plan.items[0].main_size, 180.0);
        close(plan.items[1].main_size, 120.0);
        assert_eq!(plan.lines[0].freeze_iterations, 2);
    }

    #[test]
    fn opposing_min_max_violations_cancel_and_freeze_together() {
        let mut a = item(100.0);
        a.grow = 1.0;
        a.min_size = 160.0;
        let mut b = item(100.0);
        b.grow = 1.0;
        b.max_size = Some(140.0);
        let plan = resolve_main(&config(300.0), &[a, b]);
        close(plan.items[0].main_size, 160.0);
        close(plan.items[1].main_size, 140.0);
        assert_eq!(plan.lines[0].freeze_iterations, 1);
    }

    #[test]
    fn partial_grow_and_partial_shrink_leave_requested_free_space() {
        let mut a = item(100.0);
        a.grow = 0.25;
        a.shrink = 0.25;
        let grown = resolve_main(&config(300.0), &[a.clone(), a.clone()]);
        close(grown.items[0].main_size, 125.0);
        close(grown.items[1].main_size, 125.0);
        let shrunk = resolve_main(&config(100.0), &[a.clone(), a]);
        close(shrunk.items[0].main_size, 75.0);
        close(shrunk.items[1].main_size, 75.0);
    }

    #[test]
    fn hypothetical_size_selects_lines_before_flexing() {
        let mut config = config(200.0);
        config.wrap = FlexWrap::Wrap;
        config.main_gap = 10.0;
        let mut a = item(40.0);
        a.min_size = 100.0;
        let plan = resolve_main(&config, &[a, item(100.0), item(0.0)]);
        assert_eq!(plan.lines.len(), 2);
        assert_eq!(plan.lines[0].items, 0..1);
        assert_eq!(plan.lines[1].items, 1..3);
    }

    #[test]
    fn outer_insets_and_margins_are_not_flexed() {
        let mut a = item(100.0);
        a.main_inset = 20.0;
        a.main_margin = [Some(10.0), Some(10.0)];
        let plan = resolve_main(&config(220.0), &[a.clone(), a]);
        close(plan.items[0].main_size, 70.0);
        close(plan.items[0].main_position, 10.0);
        close(plan.items[1].main_position, 120.0);
    }

    #[test]
    fn negative_inner_base_for_border_box_zero_is_preserved_until_clamp() {
        let mut a = item(-20.0);
        a.main_inset = 20.0;
        a.grow = 1.0;
        let plan = resolve_main(&config(200.0), &[a.clone(), a]);
        close(plan.items[0].main_size, 80.0);
        close(plan.items[1].main_size, 80.0);
    }

    #[test]
    fn main_auto_margins_take_space_before_justification() {
        let mut config = config(300.0);
        config.justify_content = Alignment::Center;
        let mut b = item(50.0);
        b.main_margin = [None, None];
        let plan = resolve_main(&config, &[item(50.0), b]);
        close(plan.items[0].main_position, 0.0);
        close(plan.items[1].main_position, 150.0);
        close(plan.items[1].main_margin[0], 100.0);
        close(plan.items[1].main_margin[1], 100.0);
    }

    #[test]
    fn stable_order_and_reverse_geometry_do_not_reverse_paint_order() {
        let mut config = config(300.0);
        config.direction = FlexDirection::RowReverse;
        let mut a = item(50.0);
        a.order = 2;
        let mut b = item(50.0);
        b.order = -1;
        let plan = resolve_main(&config, &[a, b, item(50.0)]);
        assert_eq!(plan.order, vec![1, 2, 0]);
        close(plan.items[1].main_position, 250.0);
        close(plan.items[2].main_position, 200.0);
        close(plan.items[0].main_position, 150.0);
    }

    #[test]
    fn row_rtl_and_reverse_cancel_and_start_is_not_flex_start() {
        let mut config = config(300.0);
        config.rtl = true;
        let plan = resolve_main(&config, &[item(50.0), item(50.0)]);
        close(plan.items[0].main_position, 250.0);
        config.direction = FlexDirection::RowReverse;
        let reversed = resolve_main(&config, &[item(50.0), item(50.0)]);
        close(reversed.items[0].main_position, 0.0);
        config.justify_content = Alignment::Start;
        let start = resolve_main(&config, &[item(50.0), item(50.0)]);
        close(start.items[0].main_position, 200.0);
    }

    #[test]
    fn column_wrap_and_column_reverse_use_same_axis_algorithm() {
        let mut config = config(100.0);
        config.direction = FlexDirection::ColumnReverse;
        config.wrap = FlexWrap::Wrap;
        config.main_gap = 10.0;
        config.cross_gap = 5.0;
        let plan = cross(&config, &[item(40.0), item(40.0), item(40.0)], &[20.0; 3]);
        assert_eq!(plan.lines.len(), 2);
        close(plan.items[0].main_position, 60.0);
        close(plan.items[1].main_position, 10.0);
        close(plan.items[2].cross_position, 25.0);
    }

    #[test]
    fn indefinite_main_axis_has_no_flexing_or_wrapping() {
        let mut config = Config {
            wrap: FlexWrap::Wrap,
            ..Config::default()
        };
        config.main_gap = 10.0;
        let mut a = item(100.0);
        a.grow = 3.0;
        let plan = resolve_main(&config, &[a, item(50.0)]);
        assert_eq!(plan.lines.len(), 1);
        close(plan.main_size, 160.0);
        close(plan.items[0].main_size, 100.0);
    }

    #[test]
    fn column_line_collection_is_independent_from_used_minimum_height() {
        let config = Config {
            main_size: AvailableSize::Definite(100.0),
            collection_size: Some(AvailableSize::Indefinite),
            direction: FlexDirection::Column,
            wrap: FlexWrap::Wrap,
            ..Config::default()
        };
        let items = vec![
            Item {
                shrink: 0.0,
                ..item(40.0)
            };
            4
        ];
        let plan = resolve_main(&config, &items);
        assert_eq!(plan.lines.len(), 1);
        close(plan.main_size, 100.0);
        close(plan.items[3].main_position, 120.0);
        assert!(!plan.truncated);
    }

    #[test]
    fn column_maximum_collects_lines_before_auto_used_height() {
        let config = Config {
            main_size: AvailableSize::Indefinite,
            collection_size: Some(AvailableSize::Definite(100.0)),
            direction: FlexDirection::Column,
            wrap: FlexWrap::Wrap,
            ..Config::default()
        };
        let items = vec![
            Item {
                shrink: 0.0,
                ..item(40.0)
            };
            4
        ];
        let plan = resolve_main(&config, &items);
        assert_eq!(plan.lines.len(), 2);
        close(plan.main_size, 80.0);
        close(plan.items[1].main_position, 40.0);
        close(plan.items[2].main_position, 0.0);
        assert!(!plan.truncated);
    }

    #[test]
    fn single_line_stretch_obeys_cross_constraints_and_insets() {
        let config = Config {
            cross_size: AvailableSize::Definite(100.0),
            ..config(200.0)
        };
        let mut a = item(50.0);
        a.cross_inset = 20.0;
        a.max_cross = Some(60.0);
        let plan = cross(&config, &[a, item(50.0)], &[40.0, 30.0]);
        close(plan.items[0].cross_size, 60.0);
        close(plan.items[1].cross_size, 100.0);
        close(plan.lines[0].cross_size, 100.0);
    }

    #[test]
    fn baseline_group_includes_ascent_descent_and_margins() {
        let config = Config {
            align_items: Alignment::Baseline,
            ..config(200.0)
        };
        let mut a = item(50.0);
        a.cross_margin = [Some(2.0), Some(3.0)];
        let items = [a, item(50.0)];
        let mut plan = resolve_main(&config, &items);
        resolve_cross(
            &config,
            &items,
            &[
                CrossMeasurement {
                    border_size: 20.0,
                    baseline: Some(15.0),
                },
                CrossMeasurement {
                    border_size: 30.0,
                    baseline: Some(10.0),
                },
            ],
            &mut plan,
        );
        close(plan.lines[0].cross_size, 37.0);
        close(plan.items[0].cross_position, 2.0);
        close(plan.items[1].cross_position, 7.0);
    }

    #[test]
    fn cross_auto_margins_override_align_self_and_handle_overflow() {
        let config = Config {
            cross_size: AvailableSize::Definite(100.0),
            ..config(200.0)
        };
        let mut a = item(50.0);
        a.cross_margin = [None, None];
        a.align_self = Alignment::FlexEnd;
        let plan = cross(&config, &[a.clone()], &[20.0]);
        close(plan.items[0].cross_position, 40.0);
        close(plan.items[0].cross_size, 20.0);
        let overflow = cross(&config, &[a], &[120.0]);
        close(overflow.items[0].cross_position, 0.0);
        close(overflow.items[0].cross_margin[1], -20.0);
    }

    #[test]
    fn wrapped_lines_support_content_distribution_and_wrap_reverse() {
        let mut config = Config {
            wrap: FlexWrap::WrapReverse,
            cross_size: AvailableSize::Definite(100.0),
            align_items: Alignment::FlexStart,
            align_content: Alignment::SpaceBetween,
            ..config(100.0)
        };
        let items = [item(60.0), item(60.0)];
        let plan = cross(&config, &items, &[20.0, 30.0]);
        close(plan.items[0].cross_position, 80.0);
        close(plan.items[1].cross_position, 0.0);
        config.align_content = Alignment::Stretch;
        let stretched = cross(&config, &items, &[20.0, 30.0]);
        close(stretched.lines[0].cross_size, 45.0);
        close(stretched.lines[1].cross_size, 55.0);
    }

    #[test]
    fn cross_alignment_and_column_rtl_use_physical_low_positions() {
        let config = Config {
            direction: FlexDirection::Column,
            rtl: true,
            cross_size: AvailableSize::Definite(100.0),
            align_items: Alignment::FlexStart,
            ..config(200.0)
        };
        let plan = cross(&config, &[item(50.0)], &[20.0]);
        close(plan.items[0].cross_position, 80.0);
    }

    #[test]
    fn hostile_values_remain_finite_and_item_prefix_is_bounded() {
        let mut items = vec![item(1.0); MAX_FLEX_ITEMS + 10];
        items[0].base_size = f32::INFINITY;
        items[1].grow = f32::NAN;
        items[2].main_margin = [Some(f32::NEG_INFINITY), None];
        let config = Config {
            main_gap: f32::NAN,
            ..config(100.0)
        };
        let plan = cross(&config, &items, &vec![1.0; items.len()]);
        assert_eq!(plan.items.len(), MAX_FLEX_ITEMS);
        assert!(plan.truncated);
        for placement in &plan.items {
            assert!(
                [
                    placement.main_size,
                    placement.main_position,
                    placement.cross_size,
                    placement.cross_position
                ]
                .into_iter()
                .all(f32::is_finite)
            );
        }
        assert!(
            plan.lines
                .iter()
                .all(|line| line.freeze_iterations <= MAX_FLEX_ITEMS + 1)
        );
        assert!(plan.work <= MAX_FLEX_WORK);
    }

    #[test]
    fn numeric_work_counts_main_and_cross_visits() {
        let config = config(100.0);
        let a = Item {
            shrink: 0.0,
            ..item(50.0)
        };
        let items = [a.clone(), a];
        let mut plan = resolve_main(&config, &items);
        // Collection + five initial freeze scans + three placement scans.
        // The non-growing, non-shrinking items freeze before iteration one.
        assert_eq!(plan.work, 18);
        resolve_cross(
            &config,
            &items,
            &[CrossMeasurement {
                border_size: 10.0,
                baseline: None,
            }; 2],
            &mut plan,
        );
        // Natural measurements, one natural line, final item and line visits.
        assert_eq!(plan.work, 24);
        assert!(!plan.truncated);
    }

    #[test]
    fn freeze_work_exhaustion_keeps_last_complete_clamped_targets() {
        let mut a = item(0.0);
        a.grow = 1.0;
        a.max_size = Some(50.0);
        let mut b = item(0.0);
        b.grow = 1.0;
        let mut work = 0;
        let (targets, iterations, truncated) = flex_line(&[0, 1], &[a, b], 300.0, &mut work, 16);
        assert_eq!(targets, [50.0, 150.0]);
        assert_eq!(iterations, 1);
        assert_eq!(work, 18);
        assert!(truncated);
    }

    #[test]
    fn saturated_main_and_cross_coordinates_report_truncation() {
        let a = Item {
            shrink: 0.0,
            ..item(600_000.0)
        };
        let main = resolve_main(&config(100.0), &[a.clone(), a.clone(), a]);
        assert!(main.truncated);
        assert_eq!(main.items[2].main_position, MAX_FLEX_COORD);
        let config = Config {
            wrap: FlexWrap::Wrap,
            ..config(100.0)
        };
        let cross = cross(
            &config,
            &[item(80.0), item(80.0), item(80.0)],
            &[600_000.0; 3],
        );
        assert!(cross.truncated);
        assert_eq!(cross.cross_size, MAX_FLEX_COORD);
        assert_eq!(cross.items[2].cross_position, MAX_FLEX_COORD);
    }

    #[test]
    fn wrap_reverse_baselines_anchor_by_descent_after_line_stretch() {
        let config = Config {
            main_size: AvailableSize::Definite(200.0),
            cross_size: AvailableSize::Definite(100.0),
            wrap: FlexWrap::WrapReverse,
            align_items: Alignment::Baseline,
            align_content: Alignment::Stretch,
            ..Config::default()
        };
        let items = [item(80.0), item(80.0)];
        let mut plan = resolve_main(&config, &items);
        resolve_cross(
            &config,
            &items,
            &[
                CrossMeasurement {
                    border_size: 20.0,
                    baseline: Some(15.0),
                },
                CrossMeasurement {
                    border_size: 40.0,
                    baseline: Some(31.0),
                },
            ],
            &mut plan,
        );
        close(plan.lines[0].cross_size, 100.0);
        close(plan.lines[0].baseline, 91.0);
        close(plan.items[0].cross_position, 76.0);
        close(plan.items[1].cross_position, 60.0);
    }

    #[test]
    fn supplied_work_budget_bounds_prefix_freezing_and_complete_cross_pass() {
        let config = Config {
            wrap: FlexWrap::Wrap,
            cross_size: AvailableSize::Definite(100.0),
            ..config(100.0)
        };
        let mut a = item(20.0);
        a.grow = 1.0;
        let items = vec![a; 200];
        let measurements = vec![
            CrossMeasurement {
                border_size: 10.0,
                baseline: None
            };
            items.len()
        ];
        for budget in [0, 1, 15, 16, 31, 32, 1000, MAX_FLEX_WORK] {
            let mut plan = resolve_main_with_budget(&config, &items, budget);
            assert_eq!(plan.items.len(), items.len().min(budget / 16));
            resolve_cross(&config, &items, &measurements, &mut plan);
            assert!(plan.work <= budget, "{} > {budget}", plan.work);
            assert_eq!(plan.order.len(), plan.items.len());
            assert!(
                plan.items
                    .iter()
                    .all(|item| item.cross_position.is_finite())
            );
            if budget == MAX_FLEX_WORK {
                assert!(!plan.truncated);
            } else {
                assert!(plan.truncated);
            }
        }
    }
}
