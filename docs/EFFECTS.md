# Phos linear gradients and outer shadows

The static SVG painter supports a bounded set of backgrounds and outer shadows
for real interface panels, labels and overlays. Effects use the existing layout,
paint groups, rounded geometry and clips. They do not fetch resources or change
box layout dimensions. This is a documented subset of CSS painting.

## Accepted syntax

`background-image` accepts one `linear-gradient(...)` or `none`. `background`
accepts a gradient alone, a supported solid color, or `none`; general combined
background shorthand, multiple backgrounds and image URLs remain deferred.
An independently specified `background-color` paints underneath the gradient.

A gradient has an optional direction and 2–16 comma-separated stops:

```css
background-image: linear-gradient(to right, #eef2f0, #d9e3df);
background-image: linear-gradient(125deg, rgba(20,50,40,.8) 10%, transparent 90%);
background-image: linear-gradient(to top right, red -20px, white, blue 120%);
box-shadow: 0 3px 12px 1px rgba(0,0,0,.18), 0 0 0 1px #b8c3bd;
```

Directions are `to top/right/bottom/left`, either valid pair of horizontal and
vertical sides in either order, a finite `deg` angle with magnitude at most
16,384, or unitless zero. Omitted direction is downward. Stops contain a
supported color and at most one shared length/percentage/calculation position.
Negative and greater-than-100% positions are supported. `currentcolor` gradient
stops, interpolation-space arguments, hints, multi-position stops, radial/conic
and repeating gradients are ignored as invalid complete declarations.

`box-shadow` accepts `none` or 1–4 comma-separated outer shadows. A layer has
two offset lengths, optional nonnegative blur, optional signed spread, and one
optional color before or after the lengths. Omitted color and `currentcolor`
use the final cascaded element color, independent of declaration order.
Offsets/spread have magnitude at most 4,096 CSS px; blur is 0–256 CSS px.
Shared lengths and calculations must resolve without a percentage containing
size. Percentage shadows and `inset` are unsupported. A malformed/excess layer
invalidates the complete declaration; earlier valid values and later unrelated
declarations survive. Each effect value is limited to 4,096 source bytes.

## Gradient geometry and alpha

Gradients use the border box as their deliberate positioning/painting box;
`background-origin`, `background-clip`, positioning, sizing and repeat controls
are deferred. They fill the rounded border path beneath the border. Inline
fragment gradients restart in each fragment, rather than implementing a full
CSS sliced background image across line breaks.

Angles use CSS's upward zero and clockwise positive convention. The gradient
line has length `abs(width × sin(angle)) + abs(height × cos(angle))`; corner
directions derive a box-dependent perpendicular so the neighboring corners
share the middle color. Stop fixup supplies missing endpoints, raises decreasing
explicit positions to the prior position and distributes omitted runs evenly.
Coincident positions retain hard transitions. Out-of-range stops expand the
SVG line to cover authored positions before normalization; they are not simply
clamped to zero and one.

The model uses the engine's existing 8-bit sRGBA colors. CSS premultiplies alpha
when interpolating its gradient colors, while SVG interpolates RGB and alpha
separately. The painter therefore samples the premultiplied color function and
adaptively subdivides each interval until straight SVG interpolation has at
most 1/1,024 error per normalized premultiplied channel. There are at most eight
bisections/256 leaves per interval (at most 3,855 emitted stops for 16 authored
stops). Fully transparent boundaries take neighboring nontransparent RGB;
zero-alpha duplicate stops permit different colors on adjoining intervals.
This is an explicit bounded approximation, not a new color-management system.

## Shadow geometry and painting

Outer shadows start from the rounded border box. Spread changes dimensions and
uses the CSS small-radius cubic adjustment before corner overlap normalization.
Negative spread can remove a shadow without truncating the element. Circular
radii now use exact SVG arc commands: browser comparison exposed an inherited
quadratic-corner approximation that noticeably changed hard-shadow and knockout
edges. Backgrounds, gradient fills, border paths, masks and overflow clips all
share the corrected circular path.

Blur uses a real SVG `feGaussianBlur` with standard deviation `blur / 2`, in
sRGB, and a finite support region extending three standard deviations from the
spread shape. The omitted Gaussian tail is below 0.3% of an opaque straight
edge. This bounded support and browser rasterization can differ slightly from
CSS implementations. Blur is not substituted with opacity or a solid offset.

A reusable luminance mask knocks out the original rounded border-box interior,
including when the element's background is transparent. Authored shadow order
is front-to-back; emission is reversed so the first layer is foremost. The
element paints shadows, solid background, gradient, border, then descendants.
Its own overflow clip applies to descendants after these decorations; ancestor
clips still enclose the shadows. Existing local z-index ordering and positioned
translations apply to the whole decorated primitive.

Shadows affect review ink extents, including positioned and ancestor-clipped
content, but never flex bases, grid contributions or normal box flow height.
SVG gradients, filters and masks use unique `phos-effect-N` IDs. Canonical
definitions are reused across box positions through local coordinates and
translations. No external SVG definitions or assets are required.

## Work and output bounds

The painter accepts at most 4,096 unique effect definitions, sharing the existing
16 MiB definition budget with glyph paths. Full serialized effect definitions
count toward that budget; the existing 128 MiB SVG bound remains unchanged.
Each shadow instance has at most 16,777,216 CSS-pixel surface area, with a total
67,108,864 per SVG, including knockout masks and reused filters. This limits
browser raster work as well as serialized output. Reused definitions remain
usable after the new-definition budget is exhausted. An omitted effect due to
work/output limits sets `data-phos-truncated="true"` and preserves renderable
backgrounds and content. Invalid CSS is ignored rather than reported as scene
truncation. Manually assembled scenes receive the same effect validation.

The focused suites contain six effects-model tests, five painter unit tests,
and sixteen effects integration tests. They assert stop geometry/fixup and
alpha, malformed recovery, bounds, exact box coordinates, paint order, clip
nesting, interior knockout, Gaussian parameters, resource reuse, valid unique
IDs/references, extents and explicit truncation.

```sh
cargo test --locked --lib effects::tests
cargo test --locked --lib paint::tests
cargo test --locked --test effects
cargo run --locked -- tests/render/effects.html --width 300 --output /tmp/phos-effects.svg
```

## Browser evidence and provenance

`tests/render/effects.html` is an original six-box engineering fixture with
precise absolute coordinates; no text or installed-font assumptions affect
this comparison. Its source and actual `scarlite` SVG were rendered in Chrome
154.0.8037.92 at 300 × 360 CSS px. All six border boxes match the independently
asserted geometry. SVG inspection found four gradients, three Gaussian filters,
four knockout masks and 29 normalized/sampled stops, without truncation.

Transparent-stop and out-of-range-stop interiors matched source HTML exactly.
Nonzero-alpha interiors had at most one 8-bit RGB-channel difference. After the
circular-arc correction, the whole-image mean absolute RGB-channel difference
was 0.085/255 (previously 0.383), maximum 10 (previously 154). Shadow region means
were 0.036–0.408/255. Residual differences cluster around Gaussian/antialias
edges; the renderer is not pixel-identical to Chrome. Numerical/XML assertions
are the primary oracle; raster comparison supplements them.

Review artifacts outside the repository are named
`engineering-effects-browser.png`, `engineering-effects-phos.png`,
`engineering-effects-phos.svg` and `engineering-effects-browser-comparison.json`.
No screenshots, downloaded fonts, assets or expected browser output are checked
in. All new effect code and fixture source are original under the repository's
Apache-2.0 license; no dependency was added for this stage.

## Primary specification sections

The geometry and stop rules use [CSS Images §3.1, linear gradients](https://www.w3.org/TR/css-images-3/#linear-gradients),
[§3.4.2, coloring the gradient line](https://www.w3.org/TR/css-images-3/#coloring-gradient-line),
and [§3.4.3, stop fixup](https://www.w3.org/TR/css-images-3/#color-stop-fixup).
Shadow shape, blur and order use [CSS Backgrounds and Borders §6.1](https://www.w3.org/TR/css-backgrounds-3/#box-shadow),
[§6.1.1, shadow shape](https://www.w3.org/TR/css-backgrounds-3/#shadow-shape),
[§6.1.2, blur](https://www.w3.org/TR/css-backgrounds-3/#shadow-blur), and
[§6.1.3, layering](https://www.w3.org/TR/css-backgrounds-3/#shadow-layers).
The alpha adaptation follows [SVG 2 §14.2.4.2, gradient stops](https://www.w3.org/TR/SVG2/pservers.html#GradientStops).
The supported syntax and deliberate simplifications above are narrower than
these specifications.
