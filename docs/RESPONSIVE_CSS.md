# Phos responsive CSS and bounded calculations

This is an implemented subset for static screen rendering. It does not claim
Media Queries, CSS Values, or browser conformance. The layout algorithms and
other rendering properties are described in [RENDERING.md](RENDERING.md).

## One viewport and one cascade

`Viewport { width, height: Option<f32> }` is the static screen environment.
Public rendering entry points validate each supplied dimension as finite and
between 1 and 16,384 CSS pixels. Width-only entry points preserve their existing
signatures and leave height indefinite. Explicit height determines `vh`,
`vmin`, `vmax`, initial containing-block percentage heights, and initial
absolute vertical offsets; it never means the final document must have that
height. The SVG extent is still derived from rendered content.

The additive entry points are `render_with_viewport`,
`render_bytes_with_viewport`, `render_file_with_viewport`, and
`render_url_with_viewport`. The byte and HTTP paths retain the final response's
encoding and URL metadata. `scarlite --height 600` supplies the optional height.
For direct document/style/scene callers, use
`style::compute_with_viewport(document, sheet, viewport)` and
`layout::layout_with_images_and_viewport(document, styles, images, viewport)`
with the same environment. The old `style::compute` remains a compatibility
wrapper using a 900-pixel width and indefinite height.

Rules inside supported `@media` blocks are flattened in their original source
order. Each rule retains its enclosing media lists; all enclosing lists must
match. Conditional rules enter the existing cascade, preserving importance,
style-attribute priority, specificity, rule order, and declaration order. A
selector list contributes its highest matching specificity once. The existing
font-size prepass remains responsible for `em`-dependent properties.

Both local and HTTP stylesheet loaders evaluate supported `media` attributes
on `<style>` and stylesheet `<link>` elements with this same viewport. Inactive
links are skipped before resource loading. Template content remains inert.
These conditions always use the viewport width, including when the styled
element sits in a much narrower nested container.

## Supported width media syntax

Supported media types are omitted, `screen`, and `all`, optionally prefixed by
`only`. Comma-separated alternatives are disjoined. Parenthesized features
joined by `and` are conjoined. Accepted features are:

| Form | Example | Meaning |
| --- | --- | --- |
| Minimum | `(min-width:600px)` | Width at least 600 |
| Maximum | `(max-width:600px)` | Width at most 600 |
| Exact width | `(width:600px)` | Width equals 600 |
| Comparison | `(width > 600px)`, `(600px <= width)` | `<`, `<=`, `=`, `>=`, `>` |
| Chained range | `(400px < width <= 900px)` | Both comparisons must match |

Chained ranges require both operators to point in the same direction; an
equality-only operator is not accepted in a chain. Thresholds accept `px`,
`em`, `rem`, and unitless zero. Font-relative thresholds use the initial 16px
font, independent of authored root or element font sizes. Threshold numbers
must be finite, nonnegative, and no larger than 16,384 before unit conversion.

`print`, unknown types/features, `not`, `or`, arbitrary grouped conditions,
height/orientation/resolution queries, percentage or viewport thresholds, and
calculated thresholds are outside this subset. Unsupported or malformed
alternatives do not match; a separate valid comma alternative can still match.
Malformed parenthesized lists cannot leak their enclosed rules into the
unconditional cascade. An empty query list matches the screen environment.

## Lengths and calculations

Existing `px`, `em`, `rem`, `%`, and unitless-zero lengths remain supported.
`vw` is one percent of the supplied viewport width. `vh` is one percent of an
explicit viewport height. `vmin` and `vmax` use the smaller/larger explicit
viewport dimension. Without height, declarations requiring `vh`, `vmin`, or
`vmax` are ignored and earlier valid declarations survive. Small/large/dynamic
viewport variants, logical viewport units, and other CSS length units are
deferred. Phos has no browser chrome or scrollbar geometry.

`calc()` uses a recursive parser with product-before-sum precedence and typed
intermediates. It accepts the supported length units, percentages, finite
numbers, parentheses, and nested `calc()`. Addition/subtraction combine
length-percentage values or combine scalar numbers. Multiplication requires at
least one scalar operand; division requires a nonzero scalar divisor. A final
scalar expression cannot become a length merely because it is zero. Units are
attached to their numeric token, and binary `+`/`-` require whitespace on both
sides. `*` and `/` do not require surrounding whitespace.

For example, with a 20px element font,
`calc((100% - 2em) / 2 + 3px * 2)` becomes `50% - 14px`. Font and viewport
parts resolve at computed-value time. The containing-size dependency remains
until final layout. Even `calc(100px + 0%)` requires a definite percentage
basis. Shared sizing treats an unresolved preferred percentage expression as
automatic rather than borrowing the viewport or the eventual document height.
Negative used values in nonnegative size/padding contexts clamp to zero;
negative margins and offsets remain allowed. Negative intermediate
coefficients, such as the `-14px` above, are valid.

Other math functions, variables/custom properties, constants such as `pi` or
`infinity`, length-by-length products, and division by lengths are deferred.
Invalid syntax, wrong types, overflow, and zero division reject the declaration
without changing an earlier valid value. Balanced shorthand scanning keeps a
whole calculation together in `margin`, `padding`, gaps, and other accepted
multi-value properties.

## Work limits and recovery

| Parser work/input | Limit and policy |
| --- | --- |
| Stylesheet source | 4 MiB; UTF-8 boundary-safe prefix, truncation reported |
| Rules | 8,192; bounded prefix, truncation reported |
| Total declarations | 65,536 per parsed stylesheet |
| Declaration block | 8,192 declarations; excess prefix reported |
| Declaration names/values | 128 bytes / 65,536 bytes |
| Declaration function nesting | 64; deeper declaration rejected, later balanced declarations recover |
| Selector list | 128 supported selectors per rule |
| Selector | 8,192 bytes, 32 descendant compounds, 32 classes per compound |
| Nested media blocks | 8; further enclosed rules skipped with truncation |
| Media condition | 4,096 bytes, 16 alternatives, 16 parenthesized features per alternative |
| Cascade matching/application | 8,000,000 charged work units across both passes; visits, attribute/selector bytes, and declaration bytes bounded; exhaustion reported |
| Calculation | 4,096 bytes, 32 nested functions/parentheses, 1,024 parser tokens |
| Calculation literal | Absolute value at most 16,384; non-finite literals rejected |
| Calculation intermediate | Absolute scalar/px/percent coefficient at most 16,777,216 |
| Balanced component list | 65,536 bytes, 32 nesting levels, 256 components |

Invalid values are ignored; parser-work truncation is different and produces
`data-phos-truncated="true"` on rendered SVG. The CLI reports a bounded prefix.
`style::compute_with_status` exposes cascade truncation to direct callers;
public rendering entry points propagate it to the SVG. Template content is
excluded from cascade traversal as well as resource loading.
Existing resource-request, compressed-image, decoded-image, scene, coordinate,
glyph-outline, and SVG limits remain independent and unchanged. No new URL,
network, shaping, or font dependency was introduced.

## Evidence and specification sections

`tests/responsive.rs` checks exact breakpoint geometry immediately below, at,
and above 600px; nested-container versus viewport width; importance and source
order; initial media font units; mixed calculations; indefinite percentage
height; explicit viewport units; local/HTTP conditional stylesheet loading;
template inertness; parser truncation; and actual CLI output. CSS/value unit
tests cover strict/inclusive/chained comparisons, malformed alternatives,
complete-value rejection at bounds, typed arithmetic, and nesting boundaries.

The six original `layout-*.html` sources in `tests/render` add navigation,
sidebar documentation, dashboard spanning, intrinsic-image gallery,
unequal-content comparison, and a restrained introduction. Their geometry and
paint invariants are checked by `tests/modern_fixtures.rs` at 320, 640, and 900
CSS pixels. The three new small PNG diagrams are original repository assets,
licensed under Apache-2.0; they contain no copied screenshots or third-party
image content. Existing parser corpora and the Scarlite-UI canary source are
unchanged.

Primary specifications consulted for this subset:

- [Media Queries 4 §1.3, Units](https://www.w3.org/TR/mediaqueries-4/#units), [§2.1, Combining Media Queries](https://www.w3.org/TR/mediaqueries-4/#mq-list), [§2.4.3, Range Context](https://www.w3.org/TR/mediaqueries-4/#mq-range-context), [§2.4.4, Min/Max Prefixes](https://www.w3.org/TR/mediaqueries-4/#mq-min-max), [§2.5, Combining Media Features](https://www.w3.org/TR/mediaqueries-4/#combining), [§3.2, Error Handling](https://www.w3.org/TR/mediaqueries-4/#error-handling), and [§4.1, Width](https://www.w3.org/TR/mediaqueries-4/#width).
- [CSS Values 4 §5.6, Mixing Percentages and Dimensions](https://www.w3.org/TR/css-values-4/#mixed-percentages), [§6.1.1, Font-relative Lengths](https://www.w3.org/TR/css-values-4/#font-relative-lengths), [§6.1.2, Viewport-percentage Lengths](https://www.w3.org/TR/css-values-4/#viewport-relative-lengths), [§10.1, Basic Arithmetic](https://www.w3.org/TR/css-values-4/#calc-func), [§10.8, Syntax](https://www.w3.org/TR/css-values-4/#calc-syntax), [§10.9, Type Checking](https://www.w3.org/TR/css-values-4/#calc-type-checking), [§10.11, Computed Value](https://www.w3.org/TR/css-values-4/#calc-computed-value), and [§10.12, Range Checking](https://www.w3.org/TR/css-values-4/#calc-range).

These sections define more behavior than Phos implements. In particular the
calculation type system deliberately uses scalar multiplication/division rather
than the newer general typed-arithmetic rules.

Reproduce a height-aware render with:

```sh
cargo run --locked --bin scarlite -- tests/render/layout-navigation.html --width 900 --height 600 --output /outside/repository/navigation.svg
cargo test --locked --test responsive --test modern_fixtures
```
