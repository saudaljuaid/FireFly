# Phos browser comparison and performance record

This record describes the static backend milestone's original fixtures. The
sources are engineering cases, with no interactive application or browser
chrome. Screenshot review supplements exact scene/style geometry tests.

## Browser method

Review uses Chrome 154.0.8037.92 on Windows, through Playwright's Chromium
protocol. SVGs are emitted by the actual Linux `scarlite` command under WSL,
then opened and rasterized by Chrome at their own SVG dimensions. Each source
HTML is independently opened at 320, 640, and 900 CSS pixels. Source comparison
loads the engine's bundled DejaVu Sans regular/bold and Phos CJK faces through
`@font-face`, overriding only `font-family`. Authored layout, spacing, borders,
font sizes, and line heights remain intact. Chrome screenshots have a 768px
viewport height; the width-only Phos render has indefinite height and a SVG
extent derived from content, so browser images can include extra blank space.

The final artifact inventory contains 25 SVGs opened in Chrome, with no duplicate
resource IDs, non-finite inspected text bounds, or page errors, and 23 source
HTML browser snapshots. Used fonts loaded before measurement; unused CJK faces
in the English-only canary/welcome and the font-free effects source need no
font assumptions. It includes the
unchanged start-page canary at all three
widths; `examples/welcome.html`; all six `layout-*.html` sources at all three
widths; the focused `effects.html` paint case; and a simple HTTPS render of
`https://example.com/` at 900px, both with indefinite height and `--height 600`.
Generated SVGs, browser PNGs, metrics JSON, and
performance JSON are saved outside the repository. No downloaded page,
screenshot, or generated build artifact is included as a source fixture.

## Findings

- Navigation matches the growing/shrinking group positions, baseline placement,
  wrapped research label, auto margin, padded positioned badge, and 600px
  direction change. Narrow URL line-break locations differ as described below.
- Documentation preserves the 172px sidebar and flexible main track at medium
  and wide widths, switches to one track when narrow, and matches text-block
  positions, nested fact groups, mixed-direction content, and the 78px rounded
  clip without letting clipped content increase the declared height.
- The wide dashboard source browser resolves columns to approximately
  461.328px, 230.672px, and 140px. The tested Phos summary width is 706px and
  queue x-coordinate is 740px; explicit spanning, implicit rows, and nested
  labels match. At 320px the authored gap is 10px, and at 640/900 it is 14px.
  The browser verifies that difference; the initial test oracle, which assumed
  14px at every width, was corrected without changing the source.
- Gallery images preserve landscape 3:2, portrait 2:3, and wide 2:1 ratios,
  including the image with HTML dimensions overridden by CSS `height:auto`.
  That original source exposed a renderer defect; the fix has an exact aspect
  regression assertion, and the source retained the difficult override.
- Wide comparison columns are each 276px wide in Chrome and Phos. Unequal text
  stretches the column boxes, while footer auto margins align their bottoms.
  Narrow columns stack at the explicit breakpoint. Gradient and shadow geometry
  is reviewed separately by the focused paint fixture and effect assertions.
- The focused six-box effects comparison uses no text/font assumptions. Source
  HTML and the actual SVG have exact border-box geometry, four gradients, three
  Gaussian filters, four knockout masks, and 29 normalized/sampled stops.
  Transparent-stop and out-of-range-stop interiors match exactly; nonzero-alpha
  interiors differ by at most one RGB-channel unit. The whole-image mean
  absolute channel difference is 0.085/255, maximum 10, concentrated around
  blur/antialias edges. Replacing inherited quadratic corner approximations
  with circular arcs reduced the prior mean 0.383 and maximum 154. Numerical
  geometry/XML assertions remain the primary oracle; see [EFFECTS.md](EFFECTS.md).
- The restrained introduction agrees in nested 3fr/2fr and three-column track
  geometry, image aspect ratio, bottom/right positioned annotation, RTL text,
  and narrow stacking. The existing start-page source and tested layout geometry
  remain unchanged at 320, 640, and 900px. The shared circular-arc paint fix can
  deliberately alter rounded-edge SVG representation and rasterization.

The live HTTPS page rendered successfully with final URL
`https://example.com/` and zero linked stylesheets. Its width-only render leaves
height indefinite; the additional `--height 600` render resolves its viewport
height units. The current page uses unsupported `font` shorthand,
`color-scheme`, and `light-dark()`, so the readable result retains subset font
and color defaults. Script content remains inert and is not fetched or run.

Separate source-browser probes exposed interactions that now have exact engine
regressions. An auto-height Flexbox constrained by `min-height:100px` remains
indefinite for percentage bases: two 50% column bases with 20px content stay
20px each, while explicit `height:100px` makes them 50px each. A stretched row
item makes descendant percentage height definite; an unstretched automatic
item does not. A definite column basis can make its post-flex item height
definite even when the container's height is automatic. Column wrapping uses a
maximum-height cap separately from free-space sizing; a minimum height alone
does not supply that cap. The minimum wins when min/max constraints conflict.

The Grid matrix similarly distinguishes a completed area from an automatic
unstretched item: a 100px area with a natural 20px item and 50%-height child
keeps that child 20px, while stretching the item to 100px makes the child 50px.
An item's own percentage size resolves against the completed grid area.
Replaced-image probes verify that constrained automatic axes transfer through
the intrinsic aspect ratio, including both automatic dimensions and opposing
constraints. The focused native-image source and final SVG have identical RGBA
raster output. Positioned children retain order zero among reordered Flex/Grid
items. Wrap-reverse baseline placement matches Chrome within the established
font metric difference of 0.46px. Raw browser matrices and paired images remain
outside the repository; the sources were retained and engine assertions added.

Meaningful remaining differences are retained in the evidence. Phos prefers
supported Unicode word/URL break opportunities before emergency breaks;
Chrome's `overflow-wrap:anywhere` chooses different break positions. In the
320px dashboard URL this creates one extra Phos line and moves following rows
by 24px. The layout correctly includes that line in the item's contribution;
the source was not shortened. Small vertical/text raster differences remain
from the established font leading, SVG outline versus browser rasterization,
user-agent defaults, and lack of margin collapsing. The old start page's first
narrow tile starts around y=86 in Phos versus y=67 in Chrome, an unchanged
historical default/margin difference. The engine remains a documented subset.

Initial four new sources mistakenly combined `border-style:solid` with only
one edge width. Browser inspection showed medium borders on the other edges.
All edge widths are now explicitly authored to preserve the intended
separators. The pre-correction browser screenshots remain in the external
review artifacts. Existing fixtures, parser expectations, and Scarlite-UI were
not modified for that correction.

## Timing method and interpretation

The baseline is a89508a053c5e4260ac2ee60927690718249cf58. Baseline and final use
Linux Rust stable 1.98.1 debug binaries under WSL and the same seven original
generated sources at width 640. Six alternating before/after pairs per case
exclude the first warmup pair, leaving five measured samples for each binary.
No browser, compiler, or project test was running during these final samples.
Process times include startup, parsing, resource loading, layout, SVG
generation, and writing on the mounted Windows filesystem. They are
end-to-end measurements, with visible host/filesystem variance, rather than
precise isolated layout performance guarantees. The final raw samples are in
the external `performance-final-interleaved.json` artifact; earlier cold and
intermediate measurements are retained as historical investigation records.

| Case | Baseline median ms | Final median ms |
| --- | ---: | ---: |
| 150 mixed Unicode paragraphs | 241.39 | 248.80 |
| 1,200 flexible items | 295.82 | 412.39 |
| 1,200 grid items | 249.27 | 436.31 |
| 64 alternating nested containers | 229.71 | 268.39 |
| 100,000-character uninterrupted token | 1,072.33 | 1,084.09 |
| Rejected 100,000-track/span values | 359.47 | 446.10 |
| 5,000 malformed declarations/rules | 331.88 | 366.28 |

Flexbox and Grid costs remain material and were investigated. The baseline
ignored both display modes and stacked full-width blocks; the final engine
measures intrinsic contributions, places lines/tracks, and lays out text again
at the resulting narrower item widths. Flex output falls from 5,728,292 to
5,130,621 bytes and Grid from 5,195,919 to 4,597,480 bytes, with different
geometry. Isolating 1,200 empty fixed-size flex items added about 8.7ms, while
text-bearing items added about 130ms, locating most additional work in text
measurement. Profiling then removed repeated cross-size text measurement:
the measured Flex layout stage fell from about 170ms to 66ms on that profile.
The final end-to-end timing above includes that fix.

Independent warm stage profiles locate the Grid increase in layout:
approximately 135–146ms baseline versus 303–315ms final, while painting fell
from about 60ms to 48ms. The 64-level nested input increased from about 0.43ms
to 6.15ms in layout; rejected giant repeat/span input increased total in-process
work only from about 0.68ms to 0.92ms. Its much larger process-time variance is
therefore not evidence of track expansion. Shared contribution and child
measurement caches and
explicit measurement, placement, and cascade budgets bound the additional
work. Retained-run layout reuse is a concrete future optimization opportunity;
the final table does not conceal the current costs.

Grid isolation and scaling checks found no pathological growth. At a constant
12-column configuration with unchanged cell widths, 600/1,200/2,400 text items
take about 259/551/1,106ms in layout; fixed empty items take 4.46/9.06/16.53ms.
All fit 50/100/200 implicit rows and remain untruncated. The numeric placement
and track phases take only 0.534ms at 1,200 items and 1.141ms at 2,400. Fresh
intrinsic text contributions take 50/92/192ms for the three counts; cached
repeats take 0.079/0.151/0.312ms. Intrinsic shaping and dry/final text layout
account for the main increase. The deliberately over-cap 2,400-item six-column
case instead reports truncation at 256 rows and 1,536 placed items. Raw stage,
numeric, and contribution records are retained outside the checkout.

All seven performance inputs rendered finite output with no unexpected
truncation or resource warning. Giant repeat/span syntax is rejected as an
invalid declaration, so it creates neither huge track lists nor an occupancy
matrix; later content still renders. Boundary and work-exhaustion cases are
covered separately by the locked suite and intentionally report truncation.

## Reproduction

```sh
cargo build --locked --bin scarlite
cargo run --locked --bin scarlite -- tests/render/layout-dashboard.html --width 320 --output /outside/repository/dashboard-320.svg
cargo run --locked --bin scarlite -- tests/render/layout-dashboard.html --width 900 --output /outside/repository/dashboard-900.svg
cargo run --locked --bin scarlite -- tests/render/effects.html --width 320 --output /outside/repository/effects.svg
cargo run --locked --bin scarlite -- https://example.com --width 900 --output /outside/repository/https-example.svg
cargo test --locked --test modern_fixtures --test responsive --test effects
python3 tools/measure_layout.py --binary target/debug/scarlite --output-dir /outside/repository/performance --samples 3 --width 640
```

The timing script generates the same seven original bounded sources and records
raw samples, SVG bytes, warnings, and truncation. Run it with a separately built
baseline binary for before/after measurements. It refuses an output directory
inside the checkout.

Open each SVG in an actual browser, then open its source HTML with the same
bundled font faces at the same width. Compare item/track coordinates and clip
boundaries with `tests/modern_fixtures.rs`, rather than using pixel equality as
the only oracle. All new fixture sources and raster diagrams are original
Apache-2.0 project assets; existing dependency/font licenses are unchanged.
