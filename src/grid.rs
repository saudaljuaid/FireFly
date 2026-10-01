//! Bounded, horizontal-writing Grid placement and track sizing.
//!
//! The algorithms are independent of DOM traversal, child layout and painting.
//! Contributions are outer sizes supplied by the shared intrinsic measurer.
//! Placement uses a fixed-capacity bitset: author-supplied line numbers never
//! determine an unbounded allocation. See `docs/RENDERING.md` for the subset.

use crate::sizing::AvailableSize;
use crate::style::Length;

pub const MAX_TRACKS: usize = 256;
pub const MAX_GRID_ITEMS: usize = 4096;
pub const MAX_PLACEMENT_PROBES: usize = 262_144;
pub const MAX_PLACEMENT_WORK: usize = 4_000_000;
pub const MAX_GRID_SIZING_WORK: usize = 4_000_000;
const MAX_COORD: f32 = 1_000_000.0;
const MAX_TRACK_LIST_BYTES: usize = 16_384;
const MAX_FUNCTION_DEPTH: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrackBreadth {
    Length(Length),
    Auto,
    MinContent,
    MaxContent,
    Fr(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Track {
    pub min: TrackBreadth,
    pub max: TrackBreadth,
}

impl Default for Track {
    fn default() -> Self {
        Self::auto()
    }
}

impl Track {
    pub const fn auto() -> Self {
        Self {
            min: TrackBreadth::Auto,
            max: TrackBreadth::Auto,
        }
    }

    pub fn breadth(value: TrackBreadth) -> Self {
        Self {
            min: if matches!(value, TrackBreadth::Fr(_)) {
                TrackBreadth::Auto
            } else {
                value
            },
            max: value,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GridLine {
    #[default]
    Auto,
    Line(i16),
    Span(u16),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GridLinePair {
    pub start: GridLine,
    pub end: GridLine,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GridAutoFlow {
    #[default]
    Row,
    Column,
}

fn components(value: &str, separator: Option<u8>) -> Option<Vec<&str>> {
    let mut output = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (index, byte) in value.bytes().enumerate() {
        match byte {
            b'(' => {
                depth += 1;
                if depth > MAX_FUNCTION_DEPTH {
                    return None;
                }
            }
            b')' => depth = depth.checked_sub(1)?,
            b'[' | b']' | b'\'' | b'"' | b'\\' => return None,
            _ if depth == 0
                && separator.map_or_else(|| byte.is_ascii_whitespace(), |s| byte == s) =>
            {
                let component = value[start..index].trim();
                if !component.is_empty() || separator.is_some() {
                    output.push(component);
                }
                start = index + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    let tail = value[start..].trim();
    if !tail.is_empty() || separator.is_some() {
        output.push(tail);
    }
    (output.len() <= MAX_TRACKS).then_some(output)
}

fn function<'a>(value: &'a str, name: &str) -> Option<&'a str> {
    value
        .strip_prefix(name)?
        .strip_prefix('(')?
        .strip_suffix(')')
}

fn breadth(value: &str, length: &impl Fn(&str) -> Option<Length>) -> Option<TrackBreadth> {
    match value {
        "auto" => Some(TrackBreadth::Auto),
        "min-content" => Some(TrackBreadth::MinContent),
        "max-content" => Some(TrackBreadth::MaxContent),
        _ => {
            if let Some(number) = value.strip_suffix("fr") {
                let number: f32 = number.parse().ok()?;
                return (number.is_finite() && (0.0..=16_384.0).contains(&number))
                    .then_some(TrackBreadth::Fr(number));
            }
            let value = length(value)?;
            if matches!(
                value,
                Length::Auto | Length::MinContent | Length::MaxContent
            ) {
                return None;
            }
            Some(TrackBreadth::Length(value))
        }
    }
}

fn track(value: &str, length: &impl Fn(&str) -> Option<Length>) -> Option<Track> {
    if let Some(arguments) = function(value, "minmax") {
        let arguments = components(arguments, Some(b','))?;
        let [minimum, maximum] = arguments.as_slice() else {
            return None;
        };
        let min = breadth(minimum, length)?;
        if matches!(min, TrackBreadth::Fr(_)) {
            return None;
        }
        return Some(Track {
            min,
            max: breadth(maximum, length)?,
        });
    }
    Some(Track::breadth(breadth(value, length)?))
}

/// Parse an integer-repeat track list. The caller supplies the engine's shared
/// length parser, including font-relative, viewport and calculation semantics.
/// The complete declaration is ignored on invalid syntax or limit overflow.
/// Named lines, nested repeat(), auto-repeat and fit-content() are deferred.
pub fn parse_tracks(
    value: &str,
    parse_length: impl Fn(&str) -> Option<Length>,
) -> Option<Vec<Track>> {
    if value.len() > MAX_TRACK_LIST_BYTES {
        return None;
    }
    let value = value.trim().to_ascii_lowercase();
    if value == "none" {
        return Some(Vec::new());
    }
    let parts = components(&value, None)?;
    if parts.is_empty() {
        return None;
    }
    let mut tracks = Vec::new();
    for part in parts {
        if let Some(arguments) = function(part, "repeat") {
            let arguments = components(arguments, Some(b','))?;
            let [count, list] = arguments.as_slice() else {
                return None;
            };
            let count: usize = count.parse().ok()?;
            if !(1..=MAX_TRACKS).contains(&count) {
                return None;
            }
            let repeated: Vec<_> = components(list, None)?
                .into_iter()
                .map(|value| track(value, &parse_length))
                .collect::<Option<_>>()?;
            if repeated.is_empty() || tracks.len() + repeated.len().checked_mul(count)? > MAX_TRACKS
            {
                return None;
            }
            for _ in 0..count {
                tracks.extend_from_slice(&repeated);
            }
        } else {
            tracks.push(track(part, &parse_length)?);
            if tracks.len() > MAX_TRACKS {
                return None;
            }
        }
    }
    Some(tracks)
}

pub fn parse_line(value: &str) -> Option<GridLine> {
    let value = value.trim().to_ascii_lowercase();
    if value == "auto" {
        return Some(GridLine::Auto);
    }
    let parts: Vec<_> = value.split_ascii_whitespace().collect();
    match parts.as_slice() {
        ["span"] => Some(GridLine::Span(1)),
        ["span", count] | [count, "span"] => {
            let count: u16 = count.parse().ok()?;
            (count != 0 && usize::from(count) <= MAX_TRACKS).then_some(GridLine::Span(count))
        }
        [number] => {
            let number: i16 = number.parse().ok()?;
            (number != 0 && number.unsigned_abs() <= (MAX_TRACKS + 1) as u16)
                .then_some(GridLine::Line(number))
        }
        _ => None,
    }
}

pub fn parse_placement(value: &str) -> Option<GridLinePair> {
    let parts = components(value, Some(b'/'))?;
    match parts.as_slice() {
        [start] => Some(GridLinePair {
            start: parse_line(start)?,
            end: GridLine::Auto,
        }),
        [start, end] => Some(GridLinePair {
            start: parse_line(start)?,
            end: parse_line(end)?,
        }),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlacementInput {
    pub row: GridLinePair,
    pub column: GridLinePair,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridArea {
    pub row: usize,
    pub column: usize,
    pub row_span: usize,
    pub column_span: usize,
}

#[derive(Debug, Clone)]
pub struct PlacementResult {
    /// Same indices as inputs. Missing areas indicate the explicit work/track
    /// bound prevented placement; callers must propagate `truncated`.
    pub areas: Vec<Option<GridArea>>,
    pub rows: usize,
    pub columns: usize,
    pub probes: usize,
    /// Probes plus actual bitset word reads/writes, capped at four million.
    pub work: usize,
    pub truncated: bool,
}

#[derive(Clone, Copy)]
struct AxisPlacement {
    start: Option<usize>,
    span: usize,
}

fn axis_placement(pair: GridLinePair, explicit: usize) -> Option<AxisPlacement> {
    let line = |value: i16| {
        let value = if value > 0 {
            i32::from(value) - 1
        } else {
            explicit as i32 + 1 + i32::from(value)
        };
        (0..=MAX_TRACKS as i32)
            .contains(&value)
            .then_some(value as usize)
    };
    use GridLine::{Auto, Line, Span};
    let (start, span) = match (pair.start, pair.end) {
        (Line(start), Line(end)) => {
            let start = line(start)?;
            let end = line(end)?;
            (Some(start.min(end)), start.abs_diff(end).max(1))
        }
        (Line(start), Span(span)) => (Some(line(start)?), usize::from(span)),
        (Span(span), Line(end)) => {
            let span = usize::from(span);
            (Some(line(end)?.checked_sub(span)?), span)
        }
        (Line(start), Auto) => (Some(line(start)?), 1),
        (Auto, Line(end)) => (Some(line(end)?.checked_sub(1)?), 1),
        (Span(span), _) | (_, Span(span)) => (None, usize::from(span)),
        (Auto, Auto) => (None, 1),
    };
    if span == 0 || span > MAX_TRACKS || start.is_some_and(|start| start + span > MAX_TRACKS) {
        return None;
    }
    Some(AxisPlacement { start, span })
}

struct Occupancy {
    rows: Vec<[u64; MAX_TRACKS / 64]>,
}

impl Occupancy {
    fn new() -> Self {
        Self { rows: Vec::new() }
    }

    fn mask(start: usize, end: usize, word: usize) -> u64 {
        let low = start.saturating_sub(word * 64).min(64);
        let high = end.saturating_sub(word * 64).min(64);
        if low >= high {
            0
        } else {
            (u64::MAX >> (64 - (high - low))) << low
        }
    }

    fn free(
        &self,
        major: usize,
        minor: usize,
        major_span: usize,
        minor_span: usize,
        work: &mut Work,
    ) -> bool {
        if major + major_span > MAX_TRACKS || minor + minor_span > MAX_TRACKS {
            return false;
        }
        for row in major..(major + major_span).min(self.rows.len()) {
            for word in minor / 64..=(minor + minor_span - 1) / 64 {
                if !work.spend(1) {
                    return false;
                }
                if self.rows[row][word] & Self::mask(minor, minor + minor_span, word) != 0 {
                    return false;
                }
            }
        }
        true
    }

    fn mark(
        &mut self,
        major: usize,
        minor: usize,
        major_span: usize,
        minor_span: usize,
        work: &mut Work,
    ) -> bool {
        let words = (minor + minor_span - 1) / 64 - minor / 64 + 1;
        if !work.spend(major_span * words) {
            return false;
        }
        self.rows.resize(
            self.rows.len().max(major + major_span),
            [0; MAX_TRACKS / 64],
        );
        for row in major..major + major_span {
            for word in minor / 64..=(minor + minor_span - 1) / 64 {
                self.rows[row][word] |= Self::mask(minor, minor + minor_span, word);
            }
        }
        true
    }
}

/// Sparse row/column auto-placement. Callers supply order-modified item order;
/// source DOM order and later paint ordering remain separate responsibilities.
/// Negative lines address the explicit grid. Implicit tracks before its first
/// line, dense packing, named lines and absolute grid-area positioning defer.
pub fn place_items(
    inputs: &[PlacementInput],
    explicit_rows: usize,
    explicit_columns: usize,
    flow: GridAutoFlow,
) -> PlacementResult {
    place_items_with_budget(
        inputs,
        explicit_rows,
        explicit_columns,
        flow,
        MAX_PLACEMENT_WORK,
    )
}

/// Like `place_items`, but never spends more counted work than the caller's
/// remaining render budget. The original per-call cap still applies.
pub fn place_items_with_budget(
    inputs: &[PlacementInput],
    explicit_rows: usize,
    explicit_columns: usize,
    flow: GridAutoFlow,
    remaining_work: usize,
) -> PlacementResult {
    let item_count = inputs.len().min(MAX_GRID_ITEMS);
    let mut result = PlacementResult {
        areas: vec![None; item_count],
        rows: explicit_rows.min(MAX_TRACKS),
        columns: explicit_columns.min(MAX_TRACKS),
        probes: 0,
        work: 0,
        truncated: inputs.len() > item_count
            || explicit_rows > MAX_TRACKS
            || explicit_columns > MAX_TRACKS,
    };
    let mut work = Work {
        limit: remaining_work.min(MAX_PLACEMENT_WORK),
        count: 0,
        truncated: false,
        exhausted: false,
    };
    if !work.spend(item_count) {
        result.work = work.count;
        result.truncated = true;
        return result;
    }
    let mut resolved = Vec::with_capacity(item_count);
    for input in &inputs[..item_count] {
        let row = axis_placement(input.row, explicit_rows.min(MAX_TRACKS));
        let column = axis_placement(input.column, explicit_columns.min(MAX_TRACKS));
        if row.is_none() || column.is_none() {
            result.truncated = true;
        }
        resolved.push(row.zip(column).map(|(row, column)| match flow {
            GridAutoFlow::Row => (row, column),
            GridAutoFlow::Column => (column, row),
        }));
    }
    if resolved.iter().any(Option::is_some) {
        result.rows = result.rows.max(1);
        result.columns = result.columns.max(1);
    }
    let (mut major_count, mut minor_count) = match flow {
        GridAutoFlow::Row => (result.rows, result.columns),
        GridAutoFlow::Column => (result.columns, result.rows),
    };
    let mut occupied = Occupancy::new();
    let set_area = |index: usize,
                    major: usize,
                    minor: usize,
                    a: AxisPlacement,
                    b: AxisPlacement,
                    result: &mut PlacementResult| {
        result.areas[index] = Some(match flow {
            GridAutoFlow::Row => GridArea {
                row: major,
                column: minor,
                row_span: a.span,
                column_span: b.span,
            },
            GridAutoFlow::Column => GridArea {
                row: minor,
                column: major,
                row_span: b.span,
                column_span: a.span,
            },
        });
    };
    // 1. Fully explicit items may overlap. Their occupancy still blocks auto items.
    for (index, item) in resolved.iter().enumerate() {
        let Some((a, b)) = item else { continue };
        if let (Some(major), Some(minor)) = (a.start, b.start) {
            if !occupied.mark(major, minor, a.span, b.span, &mut work) {
                break;
            }
            major_count = major_count.max(major + a.span);
            minor_count = minor_count.max(minor + b.span);
            set_area(index, major, minor, *a, *b, &mut result);
        }
    }
    // 2. Items locked to a major track use a monotonically advancing minor
    // cursor in each such track, even where earlier holes remain available.
    let mut locked_cursor = [0usize; MAX_TRACKS];
    for (index, item) in resolved.iter().enumerate() {
        let Some((a, b)) = item else { continue };
        if let (Some(major), None) = (a.start, b.start) {
            let mut minor = locked_cursor[major];
            let mut found = false;
            while minor + b.span <= MAX_TRACKS
                && result.probes < MAX_PLACEMENT_PROBES
                && !work.exhausted
            {
                result.probes += 1;
                if !work.spend(1) {
                    break;
                }
                if occupied.free(major, minor, a.span, b.span, &mut work) {
                    found = true;
                    break;
                }
                minor += 1;
            }
            if found {
                if !occupied.mark(major, minor, a.span, b.span, &mut work) {
                    break;
                }
                locked_cursor[major] = minor + b.span;
                major_count = major_count.max(major + a.span);
                minor_count = minor_count.max(minor + b.span);
                set_area(index, major, minor, *a, *b, &mut result);
            } else {
                result.truncated = true;
            }
        }
    }
    // 3. Determine implicit minor tracks before the cursor pass.
    for (_, b) in resolved.iter().flatten() {
        minor_count = minor_count.max(b.start.map_or(b.span, |start| start + b.span));
    }
    // 4. Sparse cursor pass. Definite minor positions can advance the major
    // cursor; wholly automatic items wrap at the established minor extent.
    let (mut cursor_major, mut cursor_minor) = (0usize, 0usize);
    for (index, item) in resolved.iter().enumerate() {
        let Some((a, b)) = item else { continue };
        if a.start.is_some() {
            continue;
        }
        if let Some(minor) = b.start {
            if minor < cursor_minor {
                cursor_major += 1;
            }
            cursor_minor = minor;
        }
        let mut found = false;
        while cursor_major + a.span <= MAX_TRACKS
            && result.probes < MAX_PLACEMENT_PROBES
            && !work.exhausted
        {
            if cursor_minor + b.span > minor_count {
                cursor_major += 1;
                cursor_minor = b.start.unwrap_or(0);
                continue;
            }
            result.probes += 1;
            if !work.spend(1) {
                break;
            }
            if occupied.free(cursor_major, cursor_minor, a.span, b.span, &mut work) {
                found = true;
                break;
            }
            if b.start.is_some() {
                cursor_major += 1;
            } else {
                cursor_minor += 1;
            }
        }
        if found {
            if !occupied.mark(cursor_major, cursor_minor, a.span, b.span, &mut work) {
                break;
            }
            major_count = major_count.max(cursor_major + a.span);
            set_area(index, cursor_major, cursor_minor, *a, *b, &mut result);
            // CSS leaves the cursor at the item's start; the occupied rectangle
            // makes the next search advance without introducing a dense pass.
        } else {
            result.truncated = true;
        }
    }
    match flow {
        GridAutoFlow::Row => {
            result.rows = major_count;
            result.columns = minor_count;
        }
        GridAutoFlow::Column => {
            result.rows = minor_count;
            result.columns = major_count;
        }
    }
    result.work = work.count;
    result.truncated |= work.truncated;
    result
}

/// Outer intrinsic contributions for one grid axis, including margins and
/// decorations. `minimum` carries automatic/explicit minimum-size policy and
/// can differ from min-content (for example min-width:0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contribution {
    pub start: usize,
    pub span: usize,
    pub min_content: f32,
    pub max_content: f32,
    pub minimum: f32,
}

/// Resolved content-box inputs for an item's minimum contribution. Intrinsic
/// measurement and final layout both use this policy; percentage resolution and
/// border-box conversion stay with the shared sizing caller.
#[derive(Debug, Clone, Copy, Default)]
pub struct MinimumContribution {
    pub content_min: f32,
    pub preferred: Option<f32>,
    pub min: Option<f32>,
    pub max: Option<f32>,
    pub inset: f32,
    pub margins: f32,
    pub preferred_outer: f32,
    pub overflow_hidden: bool,
}

pub fn minimum_contribution(
    spanned: &[Track],
    available: AvailableSize,
    gap: f32,
    item: MinimumContribution,
) -> f32 {
    // A definite preferred size contributes its constrained outer size even
    // when spanning flexible tracks makes the automatic minimum zero.
    if item.preferred.is_some() {
        return dimension(item.preferred_outer);
    }
    let automatic = !item.overflow_hidden
        && spanned.iter().any(|track| track.min == TrackBreadth::Auto)
        && (spanned.len() == 1
            || spanned
                .iter()
                .all(|track| !matches!(track.max, TrackBreadth::Fr(_))));
    let fixed_area = spanned
        .iter()
        .map(|track| match track.max {
            TrackBreadth::Length(value) => value.resolve_indefinite(available.definite()),
            _ => None,
        })
        .sum::<Option<f32>>()
        .map(|size| {
            dimension(
                size + dimension(gap) * spanned.len().saturating_sub(1) as f32
                    - item.inset
                    - item.margins,
            )
        });
    let natural = if automatic {
        dimension(item.content_min)
            .min(item.max.map_or(MAX_COORD, dimension))
            .min(fixed_area.unwrap_or(MAX_COORD))
    } else {
        0.0
    };
    dimension(item.min.unwrap_or(natural) + item.inset + item.margins)
}

#[derive(Debug, Clone)]
pub struct TrackSizing {
    pub sizes: Vec<f32>,
    pub extent: f32,
    pub work: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy)]
struct TrackState {
    min: TrackBreadth,
    max: TrackBreadth,
    base: f32,
    limit: f32,
    flex: f32,
    infinitely_growable: bool,
}

fn dimension(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, MAX_COORD)
    } else {
        0.0
    }
}

fn resolved_breadth(value: TrackBreadth, base: Option<f32>) -> TrackBreadth {
    match value {
        TrackBreadth::Length(length) => length
            .resolve_indefinite(base)
            .map_or(TrackBreadth::Auto, |value| {
                TrackBreadth::Length(Length::Px(dimension(value)))
            }),
        _ => value,
    }
}

fn fixed(value: TrackBreadth) -> Option<f32> {
    match value {
        TrackBreadth::Length(Length::Px(value)) => Some(dimension(value)),
        _ => None,
    }
}

fn intrinsic(value: TrackBreadth) -> bool {
    matches!(
        value,
        TrackBreadth::Auto | TrackBreadth::MinContent | TrackBreadth::MaxContent
    )
}

#[derive(Clone, Copy)]
enum SizeValue {
    Minimum,
    MinContent,
    MaxContent,
}

impl SizeValue {
    fn contribution(self, item: Contribution) -> f32 {
        dimension(match self {
            Self::Minimum => item.minimum,
            Self::MinContent => item.min_content,
            Self::MaxContent => item.max_content,
        })
    }
}

struct Work {
    limit: usize,
    count: usize,
    truncated: bool,
    exhausted: bool,
}

impl Work {
    fn spend(&mut self, count: usize) -> bool {
        if count > self.limit.saturating_sub(self.count) {
            self.count = self.limit;
            self.truncated = true;
            self.exhausted = true;
            false
        } else {
            self.count += count;
            true
        }
    }
}

/// Equal/weighted bounded water filling. Each iteration freezes at least one
/// constrained track, or distributes all remaining space and finishes.
fn water_fill(
    values: &mut [f32],
    limits: &[f32],
    weights: &[f32],
    mut extra: f32,
    work: &mut Work,
) {
    if work.exhausted {
        return;
    }
    let mut active: Vec<_> = (0..values.len())
        .filter(|&index| weights[index] > 0.0 && limits[index] > values[index])
        .collect();
    for _ in 0..=values.len() {
        if extra <= 0.001 || active.is_empty() || !work.spend(active.len()) {
            break;
        }
        let weight: f32 = active.iter().map(|&index| weights[index]).sum();
        if weight <= 0.0 || !weight.is_finite() {
            break;
        }
        let mut consumed = 0.0;
        for &index in &active {
            let addition = (extra * weights[index] / weight).min(limits[index] - values[index]);
            values[index] = dimension(values[index] + addition);
            consumed += addition;
        }
        extra = (extra - consumed).max(0.0);
        active.retain(|&index| limits[index] - values[index] > 0.001);
        if consumed <= 0.001 {
            break;
        }
    }
}

#[derive(Clone, Copy)]
enum Phase {
    IntrinsicMinimum,
    AutoConstraintMinimum,
    ContentMinimum,
    MaxContentMinimum,
    IntrinsicMaximum,
    MaxContentMaximum,
    FlexMinimum,
    FlexConstraintMinimum,
    FlexContentMinimum,
    FlexMaxContentMinimum,
}

impl Phase {
    fn flexible(self) -> bool {
        matches!(
            self,
            Self::FlexMinimum
                | Self::FlexConstraintMinimum
                | Self::FlexContentMinimum
                | Self::FlexMaxContentMinimum
        )
    }
    fn growth_limit(self) -> bool {
        matches!(self, Self::IntrinsicMaximum | Self::MaxContentMaximum)
    }

    fn affected(self, track: TrackState) -> bool {
        match self {
            Self::IntrinsicMinimum => intrinsic(track.min),
            Self::AutoConstraintMinimum => track.min == TrackBreadth::Auto,
            Self::ContentMinimum => matches!(
                track.min,
                TrackBreadth::MinContent | TrackBreadth::MaxContent
            ),
            Self::MaxContentMinimum => track.min == TrackBreadth::MaxContent,
            Self::IntrinsicMaximum => intrinsic(track.max),
            Self::MaxContentMaximum => {
                matches!(track.max, TrackBreadth::Auto | TrackBreadth::MaxContent)
            }
            Self::FlexMinimum => track.flex > 0.0 && intrinsic(track.min),
            Self::FlexConstraintMinimum => track.flex > 0.0 && track.min == TrackBreadth::Auto,
            Self::FlexContentMinimum => {
                track.flex > 0.0
                    && matches!(
                        track.min,
                        TrackBreadth::MinContent | TrackBreadth::MaxContent
                    )
            }
            Self::FlexMaxContentMinimum => {
                track.flex > 0.0 && track.min == TrackBreadth::MaxContent
            }
        }
    }
}

/// Apply planned increases simultaneously within a span group, making the
/// result independent of item order. Flexible spanning items are handled in
/// their own weighted phase; non-flexible tracks remain fixed in that phase.
fn span_phase(
    states: &mut [TrackState],
    items: &[Contribution],
    gap: f32,
    phase: Phase,
    size: SizeValue,
    work: &mut Work,
) {
    if work.exhausted {
        return;
    }
    let mut planned = vec![0.0f32; states.len()];
    for &item in items {
        if !work.spend(item.span) {
            return;
        }
        let tracks = &states[item.start..item.start + item.span];
        let growth_limit = phase.growth_limit();
        let original: Vec<_> = tracks
            .iter()
            .map(|track| {
                if growth_limit && track.limit.is_finite() {
                    track.limit
                } else {
                    track.base
                }
            })
            .collect();
        let extent = original.iter().sum::<f32>() + gap * item.span.saturating_sub(1) as f32;
        let contribution = if matches!(phase, Phase::AutoConstraintMinimum) {
            let fixed_maximum = tracks
                .iter()
                .map(|track| fixed(track.max))
                .sum::<Option<f32>>();
            size.contribution(item)
                .min(fixed_maximum.map_or(MAX_COORD, |maximum| {
                    maximum + gap * item.span.saturating_sub(1) as f32
                }))
                .max(dimension(item.minimum))
        } else {
            size.contribution(item)
        };
        let extra = (contribution - extent).max(0.0);
        if extra <= 0.001 {
            continue;
        }
        let mut weights: Vec<_> = tracks
            .iter()
            .map(|&track| {
                if !phase.affected(track) {
                    0.0
                } else if phase.flexible() {
                    track.flex
                } else {
                    1.0
                }
            })
            .collect();
        if phase.flexible() {
            let sum: f32 = weights.iter().sum();
            let count = weights.iter().filter(|&&weight| weight > 0.0).count();
            if sum < 1.0 && count > 0 {
                for weight in weights.iter_mut().filter(|weight| **weight > 0.0) {
                    *weight += (1.0 - sum) / count as f32;
                }
            }
        }
        if weights.iter().all(|&weight| weight == 0.0) {
            continue;
        }
        let limits: Vec<_> = tracks
            .iter()
            .enumerate()
            .map(|(index, track)| {
                if weights[index] == 0.0 {
                    original[index]
                } else if growth_limit {
                    if !track.limit.is_finite() || track.infinitely_growable {
                        MAX_COORD
                    } else {
                        track.limit
                    }
                } else if phase.flexible() {
                    MAX_COORD
                } else {
                    track.limit.max(track.base)
                }
            })
            .collect();
        let mut values = original.clone();
        water_fill(&mut values, &limits, &weights, extra, work);
        let remaining = (extra
            - values
                .iter()
                .zip(&original)
                .map(|(new, old)| new - old)
                .sum::<f32>())
        .max(0.0);
        if remaining > 0.001 {
            // Content requirements can exceed a minmax() growth limit. Prefer
            // tracks with an intrinsic maximum, then other affected tracks.
            let preferred = tracks
                .iter()
                .enumerate()
                .any(|(index, track)| weights[index] > 0.0 && intrinsic(track.max));
            let beyond: Vec<_> = tracks
                .iter()
                .enumerate()
                .map(|(index, track)| {
                    if weights[index] > 0.0 && (!preferred || intrinsic(track.max)) {
                        weights[index]
                    } else {
                        0.0
                    }
                })
                .collect();
            water_fill(
                &mut values,
                &vec![MAX_COORD; tracks.len()],
                &beyond,
                remaining,
                work,
            );
        }
        for (offset, (new, old)) in values.iter().zip(&original).enumerate() {
            planned[item.start + offset] = planned[item.start + offset].max(new - old);
        }
    }
    for (track, increase) in states.iter_mut().zip(planned) {
        if phase.growth_limit() {
            if matches!(phase, Phase::IntrinsicMaximum) && !track.limit.is_finite() {
                track.infinitely_growable = true;
            }
            track.limit = dimension(if track.limit.is_finite() {
                track.limit + increase
            } else {
                track.base + increase
            });
            if matches!(phase, Phase::MaxContentMaximum) {
                track.infinitely_growable = false;
            }
        } else {
            track.base = dimension(track.base + increase);
            track.limit = track.limit.max(track.base);
        }
    }
}

fn flex_fraction(states: &[TrackState], space: f32, work: &mut Work) -> f32 {
    if work.exhausted {
        return 0.0;
    }
    let mut active: Vec<_> = states.iter().map(|track| track.flex > 0.0).collect();
    for _ in 0..=states.len() {
        if !work.spend(states.len()) {
            return 0.0;
        }
        let fixed: f32 = states
            .iter()
            .zip(&active)
            .filter(|(_, active)| !**active)
            .map(|(track, _)| track.base)
            .sum();
        let factors: f32 = states
            .iter()
            .zip(&active)
            .filter(|(_, active)| **active)
            .map(|(track, _)| track.flex)
            .sum();
        let fraction = (space - fixed).max(0.0) / factors.max(1.0);
        let mut froze = false;
        for (track, active) in states.iter().zip(&mut active) {
            if *active && track.flex * fraction < track.base - 0.001 {
                *active = false;
                froze = true;
            }
        }
        if !froze {
            return dimension(fraction);
        }
    }
    0.0
}

/// Size one axis with fixed/percentage/intrinsic/minmax/fr tracks. This is a
/// deliberately finite subset of CSS Grid's initialization, intrinsic span
/// distribution, maximization, fr freezing and auto-track stretching phases.
/// Indefinite percentages act as auto here; the layout caller can rerun with
/// the resolved container size when its percentage policy requires it.
pub fn size_tracks(
    tracks: &[Track],
    available: AvailableSize,
    gap: f32,
    items: &[Contribution],
    stretch_auto: bool,
) -> TrackSizing {
    size_tracks_with_budget(
        tracks,
        available,
        gap,
        items,
        stretch_auto,
        MAX_GRID_SIZING_WORK,
    )
}

/// Budget-aware track sizing for nested callers. Every counted phase consumes
/// only its remaining allowance; exhaustion yields finite partial track sizes
/// and explicit truncation instead of doing an unpaid ancestor phase.
pub fn size_tracks_with_budget(
    tracks: &[Track],
    available: AvailableSize,
    gap: f32,
    items: &[Contribution],
    stretch_auto: bool,
    remaining_work: usize,
) -> TrackSizing {
    let count = tracks.len().min(MAX_TRACKS);
    let invalid_gap = !gap.is_finite() || gap > MAX_COORD;
    let gap = dimension(gap);
    let mut work = Work {
        limit: remaining_work.min(MAX_GRID_SIZING_WORK),
        count: 0,
        truncated: tracks.len() > count
            || items.len() > MAX_GRID_ITEMS
            || matches!(available, AvailableSize::Definite(value) if !value.is_finite() || value > MAX_COORD)
            || invalid_gap,
        exhausted: false,
    };
    if !work.spend(count + items.len().min(MAX_GRID_ITEMS)) {
        return TrackSizing {
            sizes: vec![0.0; count],
            extent: 0.0,
            work: work.count,
            truncated: true,
        };
    }
    let available_size = available.definite().map(dimension);
    let mut states: Vec<_> = tracks[..count]
        .iter()
        .map(|track| {
            let min = resolved_breadth(track.min, available_size);
            let max = resolved_breadth(track.max, available_size);
            let base = fixed(min).unwrap_or(0.0);
            TrackState {
                min,
                max,
                base,
                limit: fixed(max).unwrap_or(f32::INFINITY).max(base),
                flex: if let TrackBreadth::Fr(value) = max {
                    dimension(value).min(16_384.0)
                } else {
                    0.0
                },
                infinitely_growable: false,
            }
        })
        .collect();
    let mut contributions = Vec::new();
    for &item in items.iter().take(MAX_GRID_ITEMS) {
        if item.span == 0 || item.start >= count || item.span > count - item.start {
            work.truncated = true;
            continue;
        }
        if ![item.minimum, item.min_content, item.max_content]
            .iter()
            .all(|value| value.is_finite())
        {
            work.truncated = true;
            continue;
        }
        contributions.push(item);
    }
    contributions.sort_by_key(|item| item.span);
    // Single-track intrinsic contributions do not need the span distributor.
    for item in contributions.iter().filter(|item| item.span == 1) {
        if !work.spend(1) {
            break;
        }
        let state = &mut states[item.start];
        state.base = state.base.max(dimension(match state.min {
            TrackBreadth::Auto => {
                if matches!(
                    available,
                    AvailableSize::MinContent | AvailableSize::MaxContent
                ) {
                    item.min_content
                        .min(fixed(state.max).unwrap_or(MAX_COORD))
                        .max(item.minimum)
                } else {
                    item.minimum
                }
            }
            TrackBreadth::MinContent => item.min_content,
            TrackBreadth::MaxContent => item.max_content,
            _ => state.base,
        }));
        let limit = match state.max {
            TrackBreadth::MinContent => Some(dimension(item.min_content)),
            TrackBreadth::Auto | TrackBreadth::MaxContent => Some(dimension(item.max_content)),
            _ => None,
        };
        if let Some(limit) = limit {
            state.limit = if state.limit.is_finite() {
                state.limit.max(limit)
            } else {
                limit
            };
        }
        state.limit = state.limit.max(state.base);
    }
    let mut offset = contributions.partition_point(|item| item.span == 1);
    while offset < contributions.len() && !work.exhausted {
        let span = contributions[offset].span;
        let end = offset + contributions[offset..].partition_point(|item| item.span == span);
        let ordinary: Vec<_> = contributions[offset..end]
            .iter()
            .copied()
            .filter(|item| {
                states[item.start..item.start + item.span]
                    .iter()
                    .all(|track| track.flex == 0.0)
            })
            .collect();
        span_phase(
            &mut states,
            &ordinary,
            gap,
            Phase::IntrinsicMinimum,
            SizeValue::Minimum,
            &mut work,
        );
        if matches!(
            available,
            AvailableSize::MinContent | AvailableSize::MaxContent
        ) {
            span_phase(
                &mut states,
                &ordinary,
                gap,
                Phase::AutoConstraintMinimum,
                SizeValue::MinContent,
                &mut work,
            );
        }
        span_phase(
            &mut states,
            &ordinary,
            gap,
            Phase::ContentMinimum,
            SizeValue::MinContent,
            &mut work,
        );
        span_phase(
            &mut states,
            &ordinary,
            gap,
            Phase::MaxContentMinimum,
            SizeValue::MaxContent,
            &mut work,
        );
        span_phase(
            &mut states,
            &ordinary,
            gap,
            Phase::IntrinsicMaximum,
            SizeValue::MinContent,
            &mut work,
        );
        span_phase(
            &mut states,
            &ordinary,
            gap,
            Phase::MaxContentMaximum,
            SizeValue::MaxContent,
            &mut work,
        );
        offset = end;
    }
    if work.exhausted {
        return finish_sizing(&states, gap, &work);
    }
    let flexible: Vec<_> = contributions
        .iter()
        .copied()
        .filter(|item| {
            item.span > 1
                && states[item.start..item.start + item.span]
                    .iter()
                    .any(|track| track.flex > 0.0)
        })
        .collect();
    span_phase(
        &mut states,
        &flexible,
        gap,
        Phase::FlexMinimum,
        SizeValue::Minimum,
        &mut work,
    );
    if work.exhausted {
        return finish_sizing(&states, gap, &work);
    }
    if matches!(
        available,
        AvailableSize::MinContent | AvailableSize::MaxContent
    ) {
        span_phase(
            &mut states,
            &flexible,
            gap,
            Phase::FlexConstraintMinimum,
            SizeValue::MinContent,
            &mut work,
        );
    }
    span_phase(
        &mut states,
        &flexible,
        gap,
        Phase::FlexContentMinimum,
        SizeValue::MinContent,
        &mut work,
    );
    span_phase(
        &mut states,
        &flexible,
        gap,
        Phase::FlexMaxContentMinimum,
        SizeValue::MaxContent,
        &mut work,
    );
    if work.exhausted {
        return finish_sizing(&states, gap, &work);
    }
    for state in &mut states {
        if !state.limit.is_finite() {
            state.limit = state.base;
        }
        state.limit = state.limit.max(state.base);
    }
    let gutters = dimension(gap * count.saturating_sub(1) as f32);
    // Maximize non-flexible tracks toward their growth limits before resolving
    // flexible tracks. Indefinite auto heights use max-content track growth.
    let mut bases: Vec<_> = states.iter().map(|track| track.base).collect();
    let limits: Vec<_> = states.iter().map(|track| track.limit).collect();
    let weights: Vec<_> = states
        .iter()
        .map(|track| if track.flex == 0.0 { 1.0 } else { 0.0 })
        .collect();
    let extra = match available {
        AvailableSize::Definite(value) => {
            (dimension(value) - gutters - bases.iter().sum::<f32>()).max(0.0)
        }
        AvailableSize::MinContent => 0.0,
        AvailableSize::Indefinite | AvailableSize::MaxContent => MAX_COORD,
    };
    water_fill(&mut bases, &limits, &weights, extra, &mut work);
    for (state, base) in states.iter_mut().zip(bases) {
        state.base = base;
    }
    if work.exhausted {
        return finish_sizing(&states, gap, &work);
    }
    let fraction = if let Some(space) = available_size {
        flex_fraction(&states, (space - gutters).max(0.0), &mut work)
    } else if matches!(available, AvailableSize::MinContent) {
        0.0
    } else {
        let mut fraction = states
            .iter()
            .filter(|track| track.flex > 0.0)
            .map(|track| track.base / track.flex.max(1.0))
            .fold(0.0f32, f32::max);
        for item in &contributions {
            if work.exhausted {
                break;
            }
            let spanned = &states[item.start..item.start + item.span];
            if spanned.iter().any(|track| track.flex > 0.0) {
                fraction = fraction.max(flex_fraction(
                    spanned,
                    (dimension(item.max_content) - gap * item.span.saturating_sub(1) as f32)
                        .max(0.0),
                    &mut work,
                ));
            }
        }
        fraction
    };
    if work.exhausted {
        return finish_sizing(&states, gap, &work);
    }
    for state in &mut states {
        state.base = dimension(state.base.max(state.flex * fraction));
    }
    if stretch_auto && let Some(space) = available_size {
        let extra = (space - gutters - states.iter().map(|track| track.base).sum::<f32>()).max(0.0);
        let auto_count = states
            .iter()
            .filter(|track| track.max == TrackBreadth::Auto)
            .count();
        if auto_count > 0 {
            for state in states
                .iter_mut()
                .filter(|track| track.max == TrackBreadth::Auto)
            {
                state.base = dimension(state.base + extra / auto_count as f32);
            }
        }
    }
    finish_sizing(&states, gap, &work)
}

fn finish_sizing(states: &[TrackState], gap: f32, work: &Work) -> TrackSizing {
    let sizes: Vec<_> = states.iter().map(|track| track.base).collect();
    let extent = sizes.iter().sum::<f32>() + dimension(gap * states.len().saturating_sub(1) as f32);
    TrackSizing {
        extent: dimension(extent),
        sizes,
        work: work.count,
        truncated: work.truncated || !extent.is_finite() || extent > MAX_COORD,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn length(value: &str) -> Option<Length> {
        if value == "0" {
            return Some(Length::Px(0.0));
        }
        if let Some(value) = value.strip_suffix("px") {
            let value: f32 = value.parse().ok()?;
            return (value.is_finite() && value >= 0.0).then_some(Length::Px(value));
        }
        let value: f32 = value.strip_suffix('%')?.parse().ok()?;
        (value.is_finite() && value >= 0.0).then_some(Length::Percent(value))
    }

    fn tracks(value: &str) -> Vec<Track> {
        parse_tracks(value, length).unwrap()
    }
    fn pair(value: &str) -> GridLinePair {
        parse_placement(value).unwrap()
    }
    fn item(row: &str, column: &str) -> PlacementInput {
        PlacementInput {
            row: pair(row),
            column: pair(column),
        }
    }
    fn contribution(start: usize, span: usize, minimum: f32, min: f32, max: f32) -> Contribution {
        Contribution {
            start,
            span,
            minimum,
            min_content: min,
            max_content: max,
        }
    }
    fn close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.03, "{actual} != {expected}");
    }
    fn sizes(actual: &TrackSizing, expected: &[f32]) {
        assert_eq!(actual.sizes.len(), expected.len());
        for (&actual, &expected) in actual.sizes.iter().zip(expected) {
            close(actual, expected);
        }
        assert!(actual.work <= MAX_GRID_SIZING_WORK);
        assert!(actual.extent.is_finite());
    }

    #[test]
    fn item_minimum_policy_is_shared_across_axes_and_intrinsic_constraints() {
        let item = MinimumContribution {
            content_min: 180.0,
            inset: 12.0,
            margins: 8.0,
            ..MinimumContribution::default()
        };
        let minimum = |tracks: &str, available, item| {
            minimum_contribution(&self::tracks(tracks), available, 10.0, item)
        };
        close(minimum("1fr", AvailableSize::Definite(100.0), item), 200.0);
        close(
            minimum("1fr 1fr", AvailableSize::Definite(100.0), item),
            20.0,
        );
        close(
            minimum("minmax(0,1fr)", AvailableSize::Indefinite, item),
            20.0,
        );
        close(
            minimum(
                "auto",
                AvailableSize::Indefinite,
                MinimumContribution {
                    overflow_hidden: true,
                    ..item
                },
            ),
            20.0,
        );
        close(
            minimum(
                "auto",
                AvailableSize::Indefinite,
                MinimumContribution {
                    min: Some(40.0),
                    max: Some(30.0),
                    ..item
                },
            ),
            60.0,
        );
        close(
            minimum("minmax(auto,70px)", AvailableSize::Indefinite, item),
            70.0,
        );
        close(
            minimum("minmax(auto,50%)", AvailableSize::Indefinite, item),
            200.0,
        );
        close(
            minimum("minmax(auto,50%)", AvailableSize::Definite(100.0), item),
            50.0,
        );
        close(
            minimum(
                "1fr 1fr",
                AvailableSize::Definite(100.0),
                MinimumContribution {
                    preferred: Some(280.0),
                    preferred_outer: 300.0,
                    ..item
                },
            ),
            300.0,
        );
    }

    #[test]
    fn caller_remaining_budgets_bound_placement_and_each_track_phase() {
        let inputs = [item("1", "1"), item("auto", "auto"), item("auto", "span 2")];
        let full = place_items(&inputs, 1, 3, GridAutoFlow::Row);
        assert!(!full.truncated);
        for allowance in 0..=full.work {
            let placed = place_items_with_budget(&inputs, 1, 3, GridAutoFlow::Row, allowance);
            assert!(placed.work <= allowance);
            assert_eq!(placed.areas.len(), inputs.len());
            if allowance < full.work {
                assert!(placed.truncated);
            } else {
                assert_eq!(placed.areas, full.areas);
            }
        }
        let tracks = tracks("minmax(auto,90px) auto 1fr 2fr");
        let contributions = [
            contribution(0, 1, 15.0, 20.0, 50.0),
            contribution(1, 3, 100.0, 120.0, 250.0),
        ];
        let full = size_tracks(
            &tracks,
            AvailableSize::Definite(350.0),
            10.0,
            &contributions,
            true,
        );
        assert!(!full.truncated);
        for allowance in 0..=full.work {
            let sized = size_tracks_with_budget(
                &tracks,
                AvailableSize::Definite(350.0),
                10.0,
                &contributions,
                true,
                allowance,
            );
            assert!(sized.work <= allowance);
            assert!(sized.extent.is_finite());
            assert!(sized.sizes.iter().all(|size| size.is_finite()));
            if allowance < full.work {
                assert!(sized.truncated);
            } else {
                sizes(&sized, &full.sizes);
            }
        }
        let indefinite = size_tracks(
            &tracks,
            AvailableSize::Indefinite,
            10.0,
            &contributions,
            false,
        );
        let mut remaining = indefinite.work - 1;
        for _ in 0..32 {
            let phase = size_tracks_with_budget(
                &tracks,
                AvailableSize::Indefinite,
                10.0,
                &contributions,
                false,
                remaining,
            );
            assert!(phase.work <= remaining);
            remaining -= phase.work;
            assert!(phase.truncated);
        }
        assert_eq!(remaining, 0);
    }

    #[test]
    fn track_syntax_retains_minmax_and_integer_repeat_functions() {
        let actual = tracks("40px 1fr minmax(0, 2fr) repeat(2, min-content auto)");
        assert_eq!(actual.len(), 7);
        assert_eq!(
            actual[0],
            Track::breadth(TrackBreadth::Length(Length::Px(40.0)))
        );
        assert_eq!(
            actual[1],
            Track {
                min: TrackBreadth::Auto,
                max: TrackBreadth::Fr(1.0)
            }
        );
        assert_eq!(
            actual[2],
            Track {
                min: TrackBreadth::Length(Length::Px(0.0)),
                max: TrackBreadth::Fr(2.0)
            }
        );
        assert_eq!(actual[3], actual[5]);
        assert_eq!(actual[4], actual[6]);
        assert!(tracks("none").is_empty());
        assert_eq!(tracks("repeat(256, 0px)").len(), MAX_TRACKS);
    }

    #[test]
    fn malformed_and_hostile_track_lists_are_rejected_whole() {
        for value in [
            "",
            "repeat(0, 1fr)",
            "repeat(257, 1fr)",
            "repeat(128, 1fr auto auto)",
            "repeat(2, repeat(2, 1fr))",
            "repeat(auto-fit, 1fr)",
            "repeat(auto-fill, 20px)",
            "[named] 1fr",
            "minmax(1fr, 20px)",
            "minmax(auto)",
            "minmax(10px,,20px)",
            "-1fr",
            "inf fr",
            "NaNfr",
            "1fr ) 20px",
            "1fr minmax(0, 1fr",
            "fit-content(20px)",
            "repeat(2,)",
            "0px,,,1fr",
            "minmax(((((((((0))))))))),1fr)",
        ] {
            assert!(parse_tracks(value, length).is_none(), "accepted {value}");
        }
        assert!(parse_tracks(&" ".repeat(MAX_TRACK_LIST_BYTES + 1), length).is_none());
    }

    #[test]
    fn grid_line_and_span_syntax_is_bounded_and_conflicts_resolve() {
        assert_eq!(parse_line("2 span"), Some(GridLine::Span(2)));
        assert_eq!(parse_line("span"), Some(GridLine::Span(1)));
        for value in [
            "0", "258", "-258", "span 0", "span -1", "span 257", "header", "2/3",
        ] {
            assert!(parse_line(value).is_none(), "accepted {value}");
        }
        assert!(parse_placement("1/2/3").is_none());
        let result = place_items(
            &[
                item("1", "3 / 1"),
                item("2", "2 / 2"),
                item("3", "span 2 / span 5"),
            ],
            3,
            3,
            GridAutoFlow::Row,
        );
        assert_eq!(
            result.areas[0],
            Some(GridArea {
                row: 0,
                column: 0,
                row_span: 1,
                column_span: 2
            })
        );
        assert_eq!(result.areas[1].unwrap().column, 1);
        assert_eq!(result.areas[1].unwrap().column_span, 1);
        assert_eq!(result.areas[2].unwrap().column_span, 2);
        assert!(!result.truncated);
    }

    #[test]
    fn explicit_items_reserve_cells_before_automatic_items() {
        let result = place_items(
            &[
                item("auto", "auto"),
                item("1", "1"),
                item("auto", "auto"),
                item("auto", "span 2"),
            ],
            1,
            2,
            GridAutoFlow::Row,
        );
        assert_eq!(
            result.areas[0],
            Some(GridArea {
                row: 0,
                column: 1,
                row_span: 1,
                column_span: 1
            })
        );
        assert_eq!(result.areas[1].unwrap().column, 0);
        assert_eq!(result.areas[2].unwrap().row, 1);
        assert_eq!(result.areas[2].unwrap().column, 0);
        assert_eq!(result.areas[3].unwrap().row, 2);
        assert_eq!((result.rows, result.columns), (3, 2));
        assert!(!result.truncated);
    }

    #[test]
    fn empty_grid_has_only_its_explicit_tracks() {
        let empty = place_items(&[], 0, 0, GridAutoFlow::Row);
        assert_eq!((empty.rows, empty.columns), (0, 0));
        assert!(!empty.truncated);
        let explicit = place_items(&[], 0, 3, GridAutoFlow::Row);
        assert_eq!((explicit.rows, explicit.columns), (0, 3));
    }

    #[test]
    fn major_locked_items_expand_minor_tracks_before_cursor_placement() {
        let result = place_items(
            &[
                item("2", "auto"),
                item("1", "1"),
                item("2", "span 2"),
                item("auto", "auto"),
            ],
            1,
            2,
            GridAutoFlow::Row,
        );
        assert_eq!(result.areas[0].unwrap().column, 0);
        assert_eq!(
            result.areas[2],
            Some(GridArea {
                row: 1,
                column: 1,
                row_span: 1,
                column_span: 2
            })
        );
        assert_eq!(result.areas[3].unwrap().row, 0);
        assert_eq!(result.areas[3].unwrap().column, 1);
        assert_eq!((result.rows, result.columns), (2, 3));
    }

    #[test]
    fn column_flow_is_the_transpose_of_row_flow() {
        let row_inputs = [
            item("auto", "auto"),
            item("1", "1"),
            item("auto", "span 2"),
            item("auto", "auto"),
        ];
        let column_inputs: Vec<_> = row_inputs
            .iter()
            .map(|input| PlacementInput {
                row: input.column,
                column: input.row,
            })
            .collect();
        let row = place_items(&row_inputs, 1, 3, GridAutoFlow::Row);
        let column = place_items(&column_inputs, 3, 1, GridAutoFlow::Column);
        assert_eq!((row.rows, row.columns), (column.columns, column.rows));
        for (row, column) in row.areas.iter().zip(&column.areas) {
            let row = row.unwrap();
            let column = column.unwrap();
            assert_eq!(
                (row.row, row.column, row.row_span, row.column_span),
                (
                    column.column,
                    column.row,
                    column.column_span,
                    column.row_span
                )
            );
        }
    }

    #[test]
    fn negative_lines_reference_explicit_end_and_overlaps_are_permitted() {
        let result = place_items(
            &[
                item("1", "1 / -1"),
                item("1", "-2 / -1"),
                item("2", "span 2 / -1"),
                item("auto", "auto"),
            ],
            2,
            3,
            GridAutoFlow::Row,
        );
        assert_eq!(result.areas[0].unwrap().column_span, 3);
        assert_eq!(result.areas[1].unwrap().column, 2);
        assert_eq!(result.areas[2].unwrap().column, 1);
        assert_eq!(result.areas[3].unwrap().column, 0);
        assert_eq!(result.areas[3].unwrap().row, 1);
        assert!(!result.truncated);
        let outside = place_items(&[item("1", "-5")], 1, 3, GridAutoFlow::Row);
        assert!(outside.truncated && outside.areas[0].is_none());
    }

    #[test]
    fn end_only_backwards_spans_and_end_start_lines_resolve_without_reversal() {
        let result = place_items(
            &[
                item("1", "auto / 3"),
                item("2", "span 2 / 4"),
                item("3", "4 / 2"),
                item("4", "-4 / -1"),
                item("5", "-1"),
            ],
            5,
            4,
            GridAutoFlow::Row,
        );
        assert_eq!(result.areas[0].unwrap().column, 1);
        assert_eq!(
            (
                result.areas[1].unwrap().column,
                result.areas[1].unwrap().column_span
            ),
            (1, 2)
        );
        assert_eq!(
            (
                result.areas[2].unwrap().column,
                result.areas[2].unwrap().column_span
            ),
            (1, 2)
        );
        assert_eq!(
            (
                result.areas[3].unwrap().column,
                result.areas[3].unwrap().column_span
            ),
            (1, 3)
        );
        assert_eq!(result.areas[4].unwrap().column, 4);
        assert_eq!(result.columns, 5);
        assert!(!result.truncated);
        let outside = place_items(&[item("1", "span 3 / 2")], 1, 4, GridAutoFlow::Row);
        assert!(outside.truncated && outside.areas[0].is_none());
    }

    #[test]
    fn bitset_handles_word_boundaries_and_last_track_without_large_matrices() {
        let result = place_items(
            &[
                item("1", "64 / span 3"),
                item("1", "257 / span 1"),
                item("1", "256"),
                item("auto", "64"),
            ],
            1,
            256,
            GridAutoFlow::Row,
        );
        assert!(result.truncated);
        assert!(result.areas[1].is_none());
        assert_eq!(result.areas[2].unwrap().column, 255);
        assert_eq!(result.areas[3].unwrap().row, 1);
        assert_eq!(result.columns, MAX_TRACKS);
    }

    #[test]
    fn many_items_placement_is_bounded_and_truncation_is_explicit() {
        let inputs = vec![PlacementInput::default(); MAX_GRID_ITEMS + 1];
        let result = place_items(&inputs, 0, 16, GridAutoFlow::Row);
        assert!(result.truncated);
        assert_eq!(result.areas.len(), MAX_GRID_ITEMS);
        assert!(result.areas.iter().all(Option::is_some));
        assert_eq!((result.rows, result.columns), (256, 16));
        assert!(result.probes < 12_000);
        let overflow = place_items(&inputs[..257], 0, 1, GridAutoFlow::Row);
        assert!(overflow.truncated && overflow.areas[256].is_none());
        assert_eq!(overflow.rows, MAX_TRACKS);
    }

    #[test]
    fn large_overlapping_spans_stop_at_the_word_work_budget() {
        let inputs = vec![item("1 / span 256", "1 / span 256"); MAX_GRID_ITEMS];
        let result = place_items(&inputs, 1, 1, GridAutoFlow::Row);
        assert!(result.truncated);
        assert_eq!(result.work, MAX_PLACEMENT_WORK);
        assert_eq!(
            result.areas.iter().filter(|area| area.is_some()).count(),
            (MAX_PLACEMENT_WORK - inputs.len()) / (MAX_TRACKS * 4)
        );
        assert_eq!((result.rows, result.columns), (MAX_TRACKS, MAX_TRACKS));
    }

    #[test]
    fn mixed_fixed_and_fractional_tracks_preserve_gaps_and_ratios() {
        let actual = size_tracks(
            &tracks("100px 1fr 2fr"),
            AvailableSize::Definite(600.0),
            10.0,
            &[],
            false,
        );
        sizes(&actual, &[100.0, 160.0, 320.0]);
        close(actual.extent, 600.0);
        assert!(!actual.truncated);
    }

    #[test]
    fn flexible_tracks_freeze_at_automatic_minima() {
        let actual = size_tracks(
            &tracks("1fr 1fr"),
            AvailableSize::Definite(400.0),
            0.0,
            &[
                contribution(0, 1, 300.0, 300.0, 500.0),
                contribution(1, 1, 20.0, 20.0, 30.0),
            ],
            false,
        );
        sizes(&actual, &[300.0, 100.0]);
        let zero_min = size_tracks(
            &tracks("minmax(0, 1fr) minmax(0, 1fr)"),
            AvailableSize::Definite(200.0),
            0.0,
            &[contribution(0, 1, 300.0, 300.0, 500.0)],
            false,
        );
        sizes(&zero_min, &[100.0, 100.0]);
    }

    #[test]
    fn subunit_flex_factors_leave_the_specified_fraction_of_space() {
        let actual = size_tracks(
            &tracks(".25fr .25fr"),
            AvailableSize::Definite(400.0),
            0.0,
            &[],
            true,
        );
        sizes(&actual, &[100.0, 100.0]);
        close(actual.extent, 200.0);
        let zero = size_tracks(
            &tracks("0fr 1fr"),
            AvailableSize::Definite(200.0),
            0.0,
            &[contribution(0, 1, 20.0, 20.0, 70.0)],
            false,
        );
        sizes(&zero, &[20.0, 180.0]);
    }

    #[test]
    fn spanning_content_keeps_an_established_nonspanning_maximum() {
        let actual = size_tracks(
            &tracks("auto auto"),
            AvailableSize::Indefinite,
            0.0,
            &[
                contribution(0, 1, 10.0, 10.0, 10.0),
                contribution(0, 2, 30.0, 30.0, 100.0),
            ],
            false,
        );
        sizes(&actual, &[10.0, 90.0]);
        close(actual.extent, 100.0);
    }

    #[test]
    fn spanning_increases_are_planned_independently_of_item_order() {
        let mut contributions = [
            contribution(0, 2, 90.0, 100.0, 160.0),
            contribution(1, 2, 140.0, 150.0, 220.0),
        ];
        let first = size_tracks(
            &tracks("auto auto auto"),
            AvailableSize::Indefinite,
            5.0,
            &contributions,
            false,
        );
        contributions.reverse();
        let second = size_tracks(
            &tracks("auto auto auto"),
            AvailableSize::Indefinite,
            5.0,
            &contributions,
            false,
        );
        assert_eq!(first.sizes, second.sizes);
        assert!(first.sizes[0] + first.sizes[1] + 5.0 >= 160.0);
        assert!(first.sizes[1] + first.sizes[2] + 5.0 >= 220.0);
    }

    #[test]
    fn spanning_auto_flexible_minimum_can_overflow_a_narrow_container() {
        let actual = size_tracks(
            &tracks("80px 1fr"),
            AvailableSize::Definite(200.0),
            10.0,
            &[contribution(0, 2, 500.0, 500.0, 700.0)],
            false,
        );
        sizes(&actual, &[80.0, 410.0]);
        close(actual.extent, 500.0);
        let relaxed = size_tracks(
            &tracks("80px minmax(0,1fr)"),
            AvailableSize::Definite(200.0),
            10.0,
            &[contribution(0, 2, 500.0, 500.0, 700.0)],
            false,
        );
        sizes(&relaxed, &[80.0, 110.0]);
    }

    #[test]
    fn intrinsic_track_keywords_minmax_limits_and_auto_stretch_differ() {
        let actual = size_tracks(
            &tracks("min-content max-content minmax(20px,60px)"),
            AvailableSize::Definite(300.0),
            0.0,
            &[
                contribution(0, 1, 10.0, 40.0, 90.0),
                contribution(1, 1, 20.0, 30.0, 100.0),
            ],
            true,
        );
        sizes(&actual, &[40.0, 100.0, 60.0]);
        let auto = size_tracks(
            &tracks("auto 40px"),
            AvailableSize::Definite(300.0),
            0.0,
            &[contribution(0, 1, 20.0, 30.0, 50.0)],
            true,
        );
        sizes(&auto, &[260.0, 40.0]);
        let unstreched = size_tracks(
            &tracks("auto 40px"),
            AvailableSize::Definite(300.0),
            0.0,
            &[contribution(0, 1, 20.0, 30.0, 50.0)],
            false,
        );
        sizes(&unstreched, &[50.0, 40.0]);
        let conflicting = size_tracks(
            &tracks("minmax(100px,20px)"),
            AvailableSize::Definite(200.0),
            0.0,
            &[],
            true,
        );
        sizes(&conflicting, &[100.0]);
    }

    #[test]
    fn indefinite_percentages_and_flexible_tracks_use_content_contributions() {
        let contributions = [
            contribution(0, 1, 10.0, 20.0, 80.0),
            contribution(1, 1, 20.0, 30.0, 150.0),
        ];
        let indefinite = size_tracks(
            &tracks("50% 1fr"),
            AvailableSize::Indefinite,
            0.0,
            &contributions,
            false,
        );
        sizes(&indefinite, &[80.0, 150.0]);
        let definite = size_tracks(
            &tracks("50% 1fr"),
            AvailableSize::Definite(400.0),
            0.0,
            &contributions,
            false,
        );
        sizes(&definite, &[200.0, 200.0]);
        let ratios = size_tracks(
            &tracks("1fr 2fr"),
            AvailableSize::Indefinite,
            0.0,
            &contributions,
            false,
        );
        sizes(&ratios, &[80.0, 160.0]);
        let minimum = size_tracks(
            &tracks("auto auto"),
            AvailableSize::MinContent,
            0.0,
            &contributions,
            false,
        );
        sizes(&minimum, &[20.0, 30.0]);
    }

    #[test]
    fn intrinsic_constraints_include_spanning_min_content_with_zero_item_minimum() {
        let items = [contribution(0, 2, 0.0, 100.0, 300.0)];
        let automatic = size_tracks(
            &tracks("auto auto"),
            AvailableSize::MinContent,
            0.0,
            &items,
            false,
        );
        sizes(&automatic, &[50.0, 50.0]);
        let flexible = size_tracks(
            &tracks("1fr 1fr"),
            AvailableSize::MinContent,
            0.0,
            &items,
            false,
        );
        sizes(&flexible, &[50.0, 50.0]);
        let limited = size_tracks(
            &tracks("minmax(auto,20px) minmax(auto,30px)"),
            AvailableSize::MinContent,
            0.0,
            &items,
            false,
        );
        sizes(&limited, &[20.0, 30.0]);
        let zero_min = size_tracks(
            &tracks("minmax(0,1fr) minmax(0,1fr)"),
            AvailableSize::MinContent,
            0.0,
            &items,
            false,
        );
        sizes(&zero_min, &[0.0, 0.0]);
    }

    #[test]
    fn spanning_subunit_factors_distribute_a_fraction_and_the_remainder_equally() {
        let actual = size_tracks(
            &tracks(".2fr .4fr"),
            AvailableSize::Definite(50.0),
            0.0,
            &[contribution(0, 2, 90.0, 90.0, 90.0)],
            false,
        );
        sizes(&actual, &[36.0, 54.0]);
        close(actual.extent, 90.0);
    }

    #[test]
    fn sizing_work_and_scene_extents_remain_bounded_under_hostile_inputs() {
        let tracks = vec![Track::auto(); MAX_TRACKS];
        let items: Vec<_> = (0..MAX_GRID_ITEMS)
            .map(|index| contribution(0, 1 + index % MAX_TRACKS, 5000.0, 7000.0, 9000.0))
            .collect();
        let actual = size_tracks(&tracks, AvailableSize::Indefinite, 1.0, &items, false);
        assert!(
            actual
                .sizes
                .iter()
                .all(|value| value.is_finite() && *value >= 0.0)
        );
        assert!(actual.work <= MAX_GRID_SIZING_WORK);
        let oversize = size_tracks(
            &[Track::breadth(TrackBreadth::Length(Length::Px(MAX_COORD))); 2],
            AvailableSize::Indefinite,
            0.0,
            &[],
            false,
        );
        assert!(oversize.truncated);
        close(oversize.extent, MAX_COORD);
        let invalid = size_tracks(
            &tracks[..1],
            AvailableSize::Definite(f32::NAN),
            f32::INFINITY,
            &[
                contribution(0, 1, f32::NAN, 0.0, 0.0),
                contribution(0, 0, 0.0, 0.0, 0.0),
            ],
            false,
        );
        assert!(invalid.truncated);
        assert!(invalid.sizes.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn hostile_repeated_full_spans_exhaust_sizing_work_without_nonfinite_values() {
        let tracks = vec![Track::auto(); MAX_TRACKS];
        let items = vec![contribution(0, MAX_TRACKS, 5000.0, 7000.0, 9000.0); MAX_GRID_ITEMS];
        let actual = size_tracks(&tracks, AvailableSize::Indefinite, 1.0, &items, false);
        assert!(actual.truncated);
        assert_eq!(actual.work, MAX_GRID_SIZING_WORK);
        assert_eq!(actual.sizes.len(), MAX_TRACKS);
        assert!(
            actual
                .sizes
                .iter()
                .all(|value| value.is_finite() && *value >= 0.0)
        );
    }
}
