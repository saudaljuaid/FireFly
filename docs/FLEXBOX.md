# Phos horizontal Flexbox subset

This is the engine contract for static navigation, toolbars, sidebars, media
compositions and responsive rows. It describes a tested subset of Flexbox,
not browser conformance. Shared intrinsic sizing, fonts, resource limits and
positioning are documented in [RENDERING.md](RENDERING.md).

## Properties and values

| Property | Accepted values and behavior |
| --- | --- |
| `display` | `flex` is a block container; `inline-flex` is an atomic inline container using the existing inline baseline model. |
| `flex-direction` | `row`, `row-reverse`, `column`, `column-reverse`; horizontal writing only. |
| `flex-wrap` | `nowrap`, `wrap`, `wrap-reverse`. Lines require a definite collection limit; auto-height columns can use a resolvable maximum height. |
| `flex-grow`, `flex-shrink` | Finite numbers from 0 through 16,384. Defaults are 0 and 1. |
| `flex-basis` | Shared nonnegative lengths, percentages and calculations; `auto`, `min-content`, `max-content`; `content` maps to the content contribution. Width intrinsic keywords use shaped contributions; column intrinsic keywords fall back to natural measured height. |
| `flex` | `none` = `0 0 auto`; `auto` = `1 1 auto`; `initial` = `0 1 auto`; one grow number = `grow 1 0%`; one basis = `1 1 basis`; `grow shrink`, `grow basis`, or `grow shrink basis`. Other orderings are ignored. |
| `order` | Integers from −32,768 through 32,767; stable ties retain source order. |
| `gap`, `row-gap`, `column-gap` | Shared nonnegative length/percentage/calculation values. `gap` takes one or two values; `normal` is zero. Horizontal percentages use definite content width; vertical percentage gaps require definite content height, otherwise zero. |
| `justify-content` | `start`, `end`, `flex-start`, `flex-end`, `center`, `space-between`, `space-around`, `space-evenly`; `stretch`/`normal` use flex-start. |
| `align-items`, `align-self` | `start`, `end`, `flex-start`, `flex-end`, `center`, `stretch`/`normal`, `baseline`/`first baseline`; `align-self:auto` inherits the container choice. Row first-baseline alignment is supported. Column baseline uses flex-start. |
| `align-content` | `start`, `end`, `flex-start`, `flex-end`, `center`, `stretch`/`normal`, `space-between`, `space-around`, `space-evenly`; applies to wrapping containers, including one collected line. |

Distributed alignment with negative free space deliberately falls back to
safe flex-start. Last baseline, vertical writing, `safe`/`unsafe` keywords,
`flex-flow`, `visibility:collapse`, and advanced intrinsic basis functions are
deferred. Invalid declarations preserve a preceding valid value and do not
prevent later valid declarations from participating in the ordinary cascade.

## Shared measurement and size resolution

The DOM integration supplies content-box bases, min/max constraints, padding
and borders, physical low/high margins, and the same shaped text contributions
used by normal flow. `AvailableSize` distinguishes definite, indefinite,
min-content and max-content queries. A percentage basis resolves only against
a definite main size. Even `0%` remains indefinite in an auto-height column
and therefore uses content; an explicit `0px` basis remains definite.

Used size, percentage definiteness and line-collection space are separate.
An auto-height column with four fixed 40 px items and `min-height:100px`
remains one 160 px column. `max-height:100px` collects two columns and uses
the natural 80 px height; explicit `height:100px` collects two columns and
uses 100 px. A maximum or minimum alone does not resolve percentage bases.
The numerical `Config.collection_size` can override collection space while
`main_size` continues to control flexing and main-axis alignment.

Final row items stretched in the cross axis provide their used height as a
definite percentage basis to descendants, including in an auto-height row.
An unstretched auto-height row item preserves unresolved descendant heights.
For columns, a definite container main size or definite flex basis makes the
post-flexing main size definite; an auto basis in an auto-height container
preserves unresolved descendant percentages even when minimum height adds
space. An auto basis taking a definite preferred height is also definite.
This distinction is part of the dry measurement cache key and final child
layout, so cached measurement cannot substitute the wrong percentage policy.

`auto` basis takes a resolvable preferred main size, otherwise natural content.
Width intrinsic bases use min-content/max-content advances from the shared
normalization, line-break and glyph-run model. Column natural height is measured
at the resolved/natural width. Border-box bases can have a negative inner base
before clamping: a zero border-box basis with padding must retain that value
during flex distribution. Padding, borders and margins themselves do not flex.

An automatic main minimum preserves unbreakable min-content in rows and natural
measured height in columns. It is capped by a resolvable preferred size and
maximum main size. `overflow:hidden` makes this automatic minimum zero;
`min-width:0`/`min-height:0` explicitly permits shrinkage. Explicit minima beat
maxima. `overflow-wrap:anywhere` contributes grapheme-level minimum widths;
`break-word` retains ordinary unbreakable intrinsic contributions. NBSP remains
unbreakable under ordinary wrapping. The engine does not implement the complete
aspect-ratio transferred automatic-minimum algorithm for replaced elements.

Intrinsic container width is a conservative sizing contribution, not final
layout: nowrap rows sum item contributions and gaps; wrapping rows use the
largest minimum and sum maxima; columns use the largest width contribution.
Definite bases are accommodated. CSS's ideal intrinsic flex-fraction algorithm
is deferred. Final definite layout still uses the actual flex sizing process.

Dry child measurements append no scene geometry, glyph primitives or SVG
resources and use a cache keyed by the relevant available/forced sizes. Final
child layout occurs once at the resolved main and cross content sizes. This
keeps nested Flexbox/Grid from consuming paint budgets during measurement.

## Inspectable numerical algorithm

`src/flex.rs` performs two phases without DOM mutation or text shaping:

1. Select the bounded DOM prefix, stable-sort by `order`, and collect lines
   from outer hypothetical sizes before flexing. Hidden/out-of-flow content and
   whitespace-only anonymous items are excluded by shared traversal. Adjacent
   text separated by comments remains one anonymous formatting item.
2. Choose growth or shrinkage from hypothetical outer sizes. Freeze zero-factor
   and initially constrained items. Resolve remaining space with grow factors
   or scaled shrink factors (`shrink × inner base`). Factor sums below one use
   partial filling. Clamp proposed sizes, accumulate min/max violations, freeze
   items by violation sign, and redistribute remaining space. Insets remain
   fixed. Every iteration freezes items; numerical cancellation has a bounded
   progress fallback.
3. Consume positive remaining space with main auto margins before justification.
   Compute physical border-start positions without reversing the source array.
4. Measure natural cross sizes at resolved main sizes. Build row baseline groups
   from the existing first-flow baseline, synthesize a bottom baseline where
   needed, determine line cross sizes, distribute lines and stretch eligible
   auto-cross-size items within constraints. Cross auto margins override
   alignment, including a deliberate writing-start overflow rule.

RTL changes the physical direction of rows and column cross placement. Reverse
directions and wrap-reverse change geometry independently. Source node IDs and
logical text stay intact. Paint uses stable order-modified source order, then
the existing local z-index grouping. Static Flexbox/Grid items participate in
supported z-index ordering. Absolute children participate as order zero,
regardless of their declared `order`, with source order breaking ties.
Relative offsets, padded absolute containing blocks,
rounded ancestor clips, images and alt fallback all use the common final child
layout path. General stacking-context promotion remains deferred.

Auto-sized images use the shared replaced-size constraint helper before final
layout. Compatible width/height minima and maxima preserve the intrinsic ratio;
opposing constraints can override it. A 128×64 image with `max-height:20px`
therefore becomes 40×20, while `min-width:100px;max-height:20px` becomes
100×20. Intrinsic contributions and final block/Flex/Grid geometry retain this
same policy; the broader Flexbox automatic-minimum transfer algorithm remains
the separate omission described above.

## Bounds and verification

Each container accepts at most 4,096 items. Lines are ranges in a linear item
array; there is no occupancy matrix. Coordinates remain finite within
±1,000,000 CSS px; saturation of final positions or line extents reports
truncation. At most `n+1` freeze iterations are allowed, and the complete
two-phase plan is additionally limited to 4,000,000 numeric item/line visits.
`Plan.work` records those visits for the shared document layout budget.
`resolve_main_with_budget` can reduce the local cap to the document's remaining
budget and selects a predictable input prefix when sixteen reserved visits per
item would exceed it. Shared DOM traversal first selects at most 4,096 eligible
items in source order, then sorts those items by `order`; a smaller numeric
budget therefore selects a prefix of that order-modified list. Direct numeric
callers select their input prefix before the numerical module sorts it.
Main work is charged before recursive cross measurement,
and the subsequent cross delta is charged separately. Sorting
and allocations are separately bounded by the item limit. On freeze work
exhaustion, the last complete targets are clamped and placed, and truncation is
explicit. Ordinary scans and cross sizing still finish within reserved capacity.

The focused suite contains 27 numeric unit tests and 35 integration tests:
unequal growth/scaled shrink, partial factors, min/max freezing, border-box zero
basis, definite/indefinite percentages, auto minima and maximum caps, row/column
wrapping, gaps, alignment, RTL/reverse/order, baselines, anonymous text, images,
Unicode, mixed nested Grid, padded overlays, clipping, static z-index and bounds.
It also pins absolute-child order-zero painting for both Flexbox and Grid and
the distinct minimum/maximum/explicit-height column collection cases, plus
compatible and contradictory auto-image constraints across three contexts.

An independent Chrome 154.0.8037.92 probe used the bundled DejaVu face to compare
normal and wrap-reverse first-baseline groups. With 20/40 px item line heights
inside a 100 px line, stretch must move the reversed group's large item to
60 px from the physical top, retaining its bottom-side descent. This exposed
and fixed a cross-start anchoring defect; numerical and integration regressions
retain it. Browser pixel rounding of font metrics can differ by subpixel
amounts from Phos's fixed glyph-run metrics. Review PNGs and geometry JSON are
outside the repository as `engineering-flex-baseline-*`.

```sh
cargo test --locked --lib flex::tests
cargo test --locked --test flex_layout
```

## Primary specification sections

The implementation was informed by [CSS Flexbox §4, flex items](https://www.w3.org/TR/css-flexbox-1/#flex-items),
[§4.3, painting](https://www.w3.org/TR/css-flexbox-1/#painting),
[§4.5, automatic minimum size](https://www.w3.org/TR/css-flexbox-1/#min-size-auto),
[§5, directions and wrapping](https://www.w3.org/TR/css-flexbox-1/#flow-order),
[§8.1, auto margins](https://www.w3.org/TR/css-flexbox-1/#auto-margins),
[§8.2, main alignment](https://www.w3.org/TR/css-flexbox-1/#justify-content-property),
[§8.3, cross alignment](https://www.w3.org/TR/css-flexbox-1/#align-items-property),
[§8.4, line alignment](https://www.w3.org/TR/css-flexbox-1/#align-content-property),
[§8.5, baselines](https://www.w3.org/TR/css-flexbox-1/#flex-baselines),
[§9, layout](https://www.w3.org/TR/css-flexbox-1/#layout-algorithm),
[§9.7, resolving flexible lengths](https://www.w3.org/TR/css-flexbox-1/#resolve-flexible-lengths),
[§9.8, definite sizes](https://www.w3.org/TR/css-flexbox-1/#definite-sizes), and
[§9.9, intrinsic sizes](https://www.w3.org/TR/css-flexbox-1/#intrinsic-sizes).
Absolute-child painting uses [CSS Display §3, display order](https://www.w3.org/TR/css-display-4/#order-property),
and [CSS Grid §9.2, absolute children](https://www.w3.org/TR/css-grid-1/#static-position).
Image constraints use [CSS 2.2 §10.3.2, replaced dimensions](https://www.w3.org/TR/CSS22/visudet.html#inline-replaced-width),
and [§10.4, replaced minimum/maximum constraint table](https://www.w3.org/TR/CSS22/visudet.html#min-max-widths).
Shared terms come from [CSS Sizing §2, terminology](https://www.w3.org/TR/css-sizing-3/#terms),
[§3.2, sizing values](https://www.w3.org/TR/css-sizing-3/#sizing-values),
[§5.2, intrinsic contributions](https://www.w3.org/TR/css-sizing-3/#intrinsic-contribution), and
[CSS Box Alignment §5.1.3, Flexbox content distribution](https://www.w3.org/TR/css-align-3/#distribution-flex), and
[§6.2.4, flex item cross alignment](https://www.w3.org/TR/css-align-3/#align-flex).
These sections contain behavior beyond the subset above; citing them does not
assert complete implementation.
