# Phos horizontal Grid subset

Grid is the static layout backend for documentation columns, dashboard panels,
galleries and comparison sections. Its placement and track algorithms are pure
functions in `src/grid.rs`; DOM integration is in `src/layout/grid_layout.rs`.
This contract describes a substantial bounded subset, not CSS Grid conformance.
Shared sizing, shaping, positioning and painting are described in
[RENDERING.md](RENDERING.md), with related contracts in
[FLEXBOX.md](FLEXBOX.md) and [RESPONSIVE_CSS.md](RESPONSIVE_CSS.md).

## Properties and syntax

| Property | Supported values and behavior |
| --- | --- |
| `display` | `grid` is a block container. `inline-grid` is an atomic inline container with an intrinsic width and a Grid baseline. Horizontal writing only. |
| `grid-template-columns`, `grid-template-rows` | `none`, or a bounded list of fixed/shared lengths, percentages, calculations, `auto`, `min-content`, `max-content`, nonnegative `fr`, `minmax()`, and integer `repeat()`. |
| `grid-auto-columns`, `grid-auto-rows` | The same track list syntax. A list repeats cyclically for implicit tracks after the explicit grid; the initial pattern is `auto`. |
| `grid-auto-flow` | Sparse `row` and `column`. Column flow transposes the row placement algorithm. |
| `grid-column`, `grid-row` | One line value or `start / end`. Values are `auto`, a nonzero integer, `span N`, `N span`, or `span` with an implied count of one. |
| `grid-column-start`, `grid-column-end`, `grid-row-start`, `grid-row-end` | The same individual line values. |
| `gap`, `row-gap`, `column-gap` | One/two shared nonnegative lengths, percentages or calculations; `normal` is zero. Column percentage gaps resolve against content width. Row percentage gaps require definite content height. |
| `order` | Bounded signed integers; equal values preserve source order. |
| `justify-items`, `justify-self` | `start`, `end`, `flex-start`, `flex-end`, `center`, `stretch`, `normal`; `justify-self:auto` takes the container choice. Inline-axis `baseline` falls back to start in this horizontal subset. |
| `align-items`, `align-self` | The same positional/stretch values, plus `baseline`/`first baseline` for items spanning one row. `align-self:auto` takes the container choice. |
| `justify-content`, `align-content` | `start`, `end`, `flex-start`, `flex-end`, `center`, `stretch`, `normal`, `space-between`, `space-around`, `space-evenly`. Stretch enlarges tracks with an `auto` maximum. |

`minmax(min,max)` accepts a fixed/percentage/calculated or intrinsic minimum
and any supported maximum, including `fr`. A flexible minimum is invalid.
If a resolved fixed maximum is smaller than its minimum, the minimum wins.
`repeat(N,list)` expands an integer count into the track list; expansion stays
within 256 tracks. Repeat nested inside repeat, named lines, quoted area names,
`fit-content()`, `auto-fit` and `auto-fill` are deferred. `grid`,
`grid-template`, `grid-template-areas` and `grid-area` shorthands are deferred.
Invalid track/placement declarations are ignored as whole declarations; later
valid declarations still participate in the existing cascade.

Positive lines count from the explicit grid's start. Negative lines count back
from its end, before implicit tracks are appended. Numeric lines are bounded to
absolute values of 257; spans range from 1 through 256. Equal start/end lines
produce a one-track span; backwards resolved lines exchange endpoints. If both
ends specify a span, the end span is ignored. Placements requiring implicit
tracks before the first explicit line are deferred and report truncation.

## Placement before sizing

The shared formatting-item traversal creates stable order-modified items,
including anonymous contiguous text; whitespace-only text, hidden content,
inert templates and absolute descendants do not occupy cells. Reordering
preserves the DOM and shaped text's logical order.

Placement first reserves fully explicit rectangles, including permitted
overlaps. It then places items with a definite major axis, determines the
minor-axis implicit extent, and advances a sparse cursor through remaining
items. A minor-axis position can be fixed while the cursor searches the major
axis. Searches stop at the track/probe/work limits. `column` flow uses the same
algorithm with transposed axes. Placement returns inspectable areas and axis
counts before any track is measured; it never constructs an attacker-sized
matrix.

## Contributions and track sizing

`AvailableSize` distinguishes definite, indefinite, min-content and max-content
queries. A fixed number is not interchangeable with an intrinsic constraint.
`AxisSpace` also carries a separate percentage basis. An automatic Grid height
constrained by `min-height`/`max-height` can provide used space for fractional
distribution and alignment without becoming a definite percentage basis.
For example, two fractional rows under `min-height:100px;row-gap:10%` receive
50px each and zero percentage gap; explicit `height:100px` supplies a 10px gap
and two 45px rows. Unresolved percentage components of row tracks become `auto`
before the numeric solver receives that used height. A supported pixel-only
component of a `minmax()` track remains fixed. Percentage-dependent calculations
retain the same indefinite policy, including a syntactic `0%` dependency.
Shared intrinsic contributions use the established normalized paragraph,
Unicode break opportunities, selected fonts and shaped advances. Inline
decorations, atomic content, image dimensions/aspect ratios, nested formatting
containers and constraints participate through that same measurer.

Each axis runs these bounded phases:

1. Resolve fixed/definite percentage track minima and growth limits; other
   intrinsic minima begin at zero. A bare `1fr` means `minmax(auto,1fr)`.
2. Apply single-track minimum, min-content and max-content contributions to
   the affected track minima and growth limits.
3. Group ordinary spanning items by increasing span. Plan increases for every
   item in a group against the same track state, then apply the largest planned
   increase per track. This prevents item iteration order from changing sizing.
   Separate phases account for item minima, intrinsic constraints, min-content
   minima, max-content minima and intrinsic maximum growth.
4. Distribute flexible spanning contributions in separate weighted phases.
   Flexible tracks receive increases by their factors. When factors sum below
   one, the unassigned part of a required spanning contribution is shared
   equally among participating flexible tracks.
5. Maximize non-flexible tracks toward established growth limits. Min-content
   queries have zero extra space; indefinite/max-content queries allow growth
   toward max-content limits.
6. Find the flexible fraction using the remaining definite space after gaps
   and non-flexible tracks. Tracks whose computed share is below their base
   freeze at that base; recompute the remaining fraction. The denominator is
   at least one, so factors totaling less than one can leave unused space.
   Indefinite/max-content queries derive a common fraction from item
   max-content contributions and existing bases.
7. Stretch `auto` maximum tracks when content alignment requests stretch,
   then compute positional/distributed alignment offsets.

Water filling freezes constrained tracks and redistributes residual space.
Spanning intrinsic growth first observes growth limits, then can satisfy a
remaining content requirement beyond a limit, preferring tracks with intrinsic
maxima. Existing nonspanning growth limits are retained while spanning growth
is planned. These are practical implementations of the cited initialization,
intrinsic growth, maximization and flexible-fraction phases. They do not claim
the full specification's fit-content rules, orthogonal flows, baseline cycles,
or every conditional preference in distributing spanning excess space.

Column sizing precedes dry height measurement at the resolved grid-area width.
Measured heights then supply row contributions. A definite container height or
fully fixed spanned rows gives initial percentage-height measurement a basis;
otherwise percentages stay unresolved during that intrinsic measurement.
Final child layout receives the definite area width/height and forced content
dimensions once. Dry measurements create no paint primitives or SVG resources
and share the engine's size-keyed measurement cache.

A final area supplies the item's own percentage-height basis, including an
intrinsically sized row. Descendant percentages depend on the item's height:
a resolved preferred height or an auto height stretched into the area is
definite; an unstretched natural auto height remains indefinite even though its
measured pixel height is supplied to final layout. For a 100px row,
`align-self:start` plus auto item height and a `height:50%` child containing a
20px leaf leaves the item/child at 20px/20px. Stretch produces 100px/50px; an
explicit item height of 20px produces 20px/10px; a 50% item height produces
50px/25px. This distinction is pinned against an actual Chrome matrix.

The subset does not perform the complete column/row/column/row dependency
reruns of CSS Grid §11.1. Percentage tracks on an indefinite axis behave as
`auto` during intrinsic sizing; indefinite row percentages remain `auto` for
that sizing pass. Percentage child heights resolve against the final area for
final layout. Aspect-ratio or wrapped-column-Flex contributions that change
after row sizing are a documented continuation, rather than a claim of full
cyclic sizing support.

## Item minimums, constraints and alignment

`minimum_contribution()` is shared by intrinsic Grid measurement and final
Grid track sizing. The automatic content-based minimum applies when overflow
is visible, at least one spanned track has an `auto` minimum, and an item
spanning multiple tracks crosses no flexible maximum. Otherwise the automatic
content minimum is zero. Padding, borders and margins still contribute.
Explicit `min-width:0`/`min-height:0` likewise relaxes the content minimum.
A definite preferred size contributes its constrained outer size even when a
multi-track flexible span has a zero automatic minimum.

The content-based minimum is capped by the resolved maximum and, when every
spanned maximum is fixed, by that area minus decorations and margins.
An unresolved percentage maximum cannot supply such a cap. Explicit minima
win over maxima. Shared numeric border-box values subtract padding/borders;
`min-content` and `max-content` keywords select content dimensions even with
`box-sizing:border-box`. Intrinsic percentage padding and margins are omitted
until their containing width becomes definite. Final edges on every side
resolve against the whole grid-area width, including spanned alignment gaps.

Auto margins consume positive space before self alignment and disable stretch
on their axis. Self alignment uses the remaining area after margins and the
border box. `start`/`end` follow the horizontal container direction, and RTL
mirrors column positions without reversing DOM traversal. Distributed gaps
increase spanning areas. Positional end/center alignment can overflow when
space is negative; distributed alignment uses zero extra gap in that case.

Images preserve their intrinsic/definite transferred aspect ratio by default.
Shared replaced-element sizing applies minimum/maximum constraints before
transferring an automatic axis through the ratio, including both-auto native
images. Incompatible opposing constraints can intentionally break the ratio.
Explicit `justify-self:stretch` or `align-self:stretch` can stretch that axis.
The current shared `normal`/`stretch` computed value does not distinguish an
explicit container stretch from normal for images, so item-level explicit
stretch is the supported control. Full replaced-element automatic-minimum
transfer rules remain deferred.

First-baseline alignment uses the existing text baseline and per-row maximum
ascent, accounting for margins and a sizing shim. Multi-row baseline alignment
falls back to start. The Grid container's first baseline uses its first
occupied row, preferring a participating baseline-sharing group and otherwise
the first item in logical row-major grid order. A missing item baseline is
synthesized from its border bottom. Last-baseline alignment, vertical writing,
orthogonal synthesis and `safe`/`unsafe` keywords are deferred.

## Positioning, clipping and paint order

Items pass through the ordinary box layout and painter: shared text runs,
images and alt fallback, box constraints, relative offsets, rounded overflow
clips, absolute containing blocks, paint groups, gradients and shadows are
preserved. Positioned overlays inside padded items use the engine's existing
padding-box containing block. Absolute children do not influence placement or
track contributions. Grid line placement of absolute children and Grid's
special static-position rectangle are deferred.

Visual grid position, logical text/source order and paint order are distinct.
Stable `order` changes formatting/paint order without rewriting the DOM.
Supported static Grid item z-index enters the existing local paint ordering;
this is not full browser stacking-context promotion. Clipped descendants and
positioned descendants retain the established scene and resource limits.

## Bounds and original regression evidence

| Work or input | Bound |
| --- | --- |
| Parsed track list | 16,384 bytes; function nesting 8; 256 expanded tracks per axis |
| Numeric line/span | Absolute line 257; span 256 |
| Items per formatting container | 4,096; omitted eligible content reports truncation |
| Occupancy storage | Up to 256 rows of four 64-bit words: 8 KiB |
| Placement searches | 262,144 probes and 4,000,000 counted word/probe operations |
| One axis's numeric sizing | 4,000,000 counted track/item operations |
| Water-fill/flexible freeze | At most track-count plus one iterations per invocation |
| Shared final layout numeric work | 8,000,000 across the render; dry measurement shares it |
| Shared intrinsic numeric work | 8,000,000, plus the existing intrinsic traversal/text budget |
| Scene coordinates | Finite; dimensions/extents capped at 1,000,000 CSS pixels |

Budget-aware placement and sizing entry points receive the caller's remaining
allowance and retain the per-call caps. Initialization/input validation and
item-minimum scans reserve counted work before recursive measurement. Each
pure phase is charged immediately, before another phase or child can recurse;
zero allowance returns finite truncated geometry. Suspended ancestor layouts
cannot perform another full unpaid sizing pass after a child exhausts the
shared allowance. The counters bound the dominant counted/reserved operations;
fixed-capacity result construction and bounded scalar bookkeeping are separate.

Track/list rejection preserves preceding valid declarations. Placement or work
exhaustion keeps accepted geometry finite and sets truncation. Exceeding the
overall extent cap also reports truncation; no bound is loosened to make a
fixture pass. Existing HTML, CSS, image, resource, text, scene and SVG output
bounds remain applicable.

The 26 unit tests in `src/grid.rs` and 31 integration checks in
`tests/grid_layout.rs` are original engineering cases. They cover parse
recovery, explicit/sparse/column placement, negative lines and overlaps,
bitset boundaries, implicit patterns, fractional freezing, spanning intrinsic
growth, intrinsic constraints, shared minimum policy, nested Flex/Grid,
breakpoint track changes, baseline ordering/clipping, images, Unicode/bidi,
percentage edges, distributed gaps, z-index/order, padded positioned clips,
deep nesting and hostile work exhaustion. No external fixture, expected image,
asset or dependency was copied for these Grid cases. The image mock uses the
repository's existing original `two-pixels.png`.

Reproduce the focused checks from the repository root:

```sh
cargo test --locked --lib grid::tests
cargo test --locked --test grid_layout
```

The milestone's completed local/CI totals, pinned HTML fixture counts, browser
comparisons and timing records belong to [ENGINE_STATUS.md](ENGINE_STATUS.md).
The next Grid step is a bounded dependency rerun for percentage/aspect-ratio
and wrapped-column-Flex contributions that change after row sizing, followed
by licensed upstream Grid cases against the explicitly supported subset.

## Primary specification sections

- [CSS Grid §5.2, Sizing Grid Containers](https://www.w3.org/TR/css-grid-1/#intrinsic-sizes),
  [§6, Grid Items](https://www.w3.org/TR/css-grid-1/#grid-items),
  [§6.4, Grid Item Margins and Paddings](https://www.w3.org/TR/css-grid-1/#item-margins),
  [§6.5, Z-axis Ordering](https://www.w3.org/TR/css-grid-1/#z-order), and
  [§6.6, Automatic Minimum Size](https://www.w3.org/TR/css-grid-1/#min-size-auto).
- [CSS Grid §7.2, Explicit Track Sizing](https://www.w3.org/TR/css-grid-1/#track-sizing),
  [§7.2.3, Repeat](https://www.w3.org/TR/css-grid-1/#repeat-notation),
  [§7.2.4, Flexible Lengths](https://www.w3.org/TR/css-grid-1/#fr-unit),
  [§7.5, Implicit Grid](https://www.w3.org/TR/css-grid-1/#implicit-grids), and
  [§7.7, Automatic Flow](https://www.w3.org/TR/css-grid-1/#grid-auto-flow-property).
- [CSS Grid §8.3, Line Placement](https://www.w3.org/TR/css-grid-1/#line-placement),
  [§8.3.1, Conflict Handling](https://www.w3.org/TR/css-grid-1/#grid-placement-errors),
  [§8.4, Placement Shorthands](https://www.w3.org/TR/css-grid-1/#placement-shorthands), and
  [§8.5, Placement Algorithm](https://www.w3.org/TR/css-grid-1/#auto-placement-algo).
- [CSS Grid §10, Alignment and Spacing](https://www.w3.org/TR/css-grid-1/#alignment),
  [§10.6, Grid Container Baselines](https://www.w3.org/TR/css-grid-1/#grid-baselines), and
  [Box Alignment §9.1, Determining Baselines](https://www.w3.org/TR/css-align-3/#baseline-export).
- [CSS Grid §11.1, Grid Sizing Algorithm](https://www.w3.org/TR/css-grid-1/#algo-grid-sizing),
  [§11.4, Initialization](https://www.w3.org/TR/css-grid-1/#algo-init),
  [§11.5, Intrinsic Track Sizes](https://www.w3.org/TR/css-grid-1/#algo-content),
  [§11.5.1, Spanning Distribution](https://www.w3.org/TR/css-grid-1/#algo-spanning-items),
  [§11.6, Maximization](https://www.w3.org/TR/css-grid-1/#algo-grow-tracks),
  [§11.7, Flexible Tracks](https://www.w3.org/TR/css-grid-1/#algo-flex-tracks), and
  [§11.8, Auto Stretch](https://www.w3.org/TR/css-grid-1/#algo-stretch).
- [CSS Sizing §3.2, Sizing Values](https://www.w3.org/TR/css-sizing-3/#sizing-values),
  [§3.3, Box Sizing](https://www.w3.org/TR/css-sizing-3/#box-sizing),
  [§5.1, Intrinsic Sizes](https://www.w3.org/TR/css-sizing-3/#intrinsic-sizes), and
  [§5.2, Intrinsic Contributions](https://www.w3.org/TR/css-sizing-3/#intrinsic-contribution).
- [Box Alignment §5.1.4, Grid Content Distribution](https://www.w3.org/TR/css-align-3/#distribution-grid),
  [§6.1, Justify Self](https://www.w3.org/TR/css-align-3/#justify-self-property),
  [§6.2, Align Self](https://www.w3.org/TR/css-align-3/#align-self-property), and
  [§8, Gaps](https://www.w3.org/TR/css-align-3/#gaps).
