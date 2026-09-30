# Phos static rendering milestone

This milestone starts from Scarlite `main` commit `907be90`. The fresh locked
baseline passed before changes: 19 library, 10 byte-input, 6 parse-error, 22
tree-construction, 2 upstream-error, 2 upstream-tokenizer, and 7 upstream-tree
tests; three diagnostic inventory tests were intentionally ignored. The pinned
upstream parser fixtures and expected outputs were not changed.

## Standards used and deliberate scope

The implementation follows the relevant models in [CSS Display 3](https://www.w3.org/TR/css-display-3/),
[CSS 2.2 visual formatting](https://www.w3.org/TR/CSS22/visuren.html),
[CSS Text 3](https://www.w3.org/TR/css-text-3/),
[CSS Backgrounds and Borders 3](https://www.w3.org/TR/css-backgrounds-3/),
[CSS Cascading 5](https://www.w3.org/TR/css-cascade-5/), and
[CSS 2.2 box dimensions](https://www.w3.org/TR/CSS2/visudet.html).
Those specifications describe more behavior than Phos implements. The matrix
below is the supported subset; unsupported values and rules are ignored.

| Area | Supported values and behavior | Limits |
| --- | --- | --- |
| Selectors | Tag, class, ID, compound and descendant selectors | No pseudo classes/elements, combinators other than descendant, or media queries |
| Cascade | User-agent defaults, linked and inline sheets in document order, style attributes, specificity, source order, inheritance and `!important` | No user stylesheets, cascade layers, custom properties, `@import`, or animations |
| Declaration syntax | Comments, escapes, quoted strings, nested functions/parentheses, semicolons inside values, and trailing `!important` | Not a general CSS Syntax implementation; malformed unterminated values are dropped to the enclosing rule |
| `display` | `block`, `inline`, `inline-block`, `none` | No flex, grid, table layout, or floats |
| Size | `width`, `height`, `min-*`, `max-*`, `box-sizing:content-box/border-box` | Intrinsic sizing is approximate for non-replaced inline-blocks |
| Edges | One-to-four-value `margin`, `padding`, `border-width`; `auto` horizontal margins | No margin collapsing; side-specific border style/color is not implemented |
| Borders | `border` width/style/color, `border-style:none/solid/dashed/dotted`, `border-color`, `border-radius` | Circular radii only; asymmetric borders are painted as straight side rectangles |
| Color | `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()`, common named colors, `transparent`; alpha in text/background/border | No full named-color table, CSS Color 4 space syntax, gradients, or background images |
| Text | `font-size`, regular/bold weight, `line-height`, `text-align:left/center/right`, `white-space:normal/nowrap/pre/pre-wrap/pre-line`, `<br>` | No bidi algorithm, hyphenation, complex shaping, or full Unicode line breaking |
| Clipping | `overflow:visible/hidden`; hidden descendants clip to a rounded padding edge | No scrolling or independent `overflow-x/y` |
| Images | HTML `<img>` PNG/JPEG; intrinsic or explicit HTML/CSS dimensions; ratio preservation; alt fallback | No GIF, WebP, SVG image decoding, srcset, or CSS background images |

Lengths accept `px`, `em`, `rem`, `%`, and unitless zero. `em` uses the
element's computed font size, while `font-size:em/%` uses its parent's size.
`rem` uses the root size. Horizontal percentages use the containing block's
content width. Percentage margins and padding on every side also use that
width, following the CSS 2.2 box model. Percentage heights and vertical
min/max heights use a containing block with a definite height; otherwise they
act as auto/unresolved. Coordinates and box sizes are bounded to finite
values. The SVG page height includes the laid-out document extent.

Mixed block and inline descendants retain document order. Inline runs form
anonymous block wrappers and line boxes, inline backgrounds form fragments across lines, and
inline-blocks contribute atomic boxes to the line baseline. Word wrapping,
long-word grapheme splitting, whitespace collapse/preservation, and `<br>`
are covered by fixtures. This is a practical static flow engine, not a full
CSS inline formatting implementation.

## Fonts and SVG

Phos measures glyph advances with the bundled DejaVu Sans regular/bold TTFs
using `fontdue`, and embeds those fonts in each SVG under the family
`Phos DejaVu`. Its `textLength` value matches the measured run width, so SVG
painting and layout allocate the same horizontal space. Grapheme clusters
keep combining marks with their base for line splitting. Missing glyphs get
deterministic advances and try DejaVu, Noto Sans CJK SC, Microsoft YaHei, Yu
Gothic, SimSun, then the viewer's generic sans serif. Coverage and
complex-script shaping depend on the viewer and installed fallback fonts.
The bundled font license is
`assets/fonts/LICENSE-dejavu.txt`.

The painter emits background and rounded-border paths before descendants,
text with escaped XML and alpha, and embedded images. `overflow:hidden`
creates SVG clip paths for descendants. Output is standalone SVG; it does
not need an external font or image request.

## Resource limits

| Resource | Limit |
| --- | ---: |
| HTML input bytes | 16 MiB |
| Open HTML elements | 256 |
| Token reprocessing | 32 steps |
| Viewport width | 1–16,384 CSS pixels |
| Linked stylesheets | 16 requests, 2 MiB each, 4 MiB combined CSS |
| Images | 16 requests, 4 MiB compressed bytes each, 16 million decoded pixels each, 64 MiB decoder allocation cap |
| HTTP(S) redirects | 5, with HTTPS-to-HTTP downgrade refused |
| Scene | Finite coordinates up to 1,000,000; a 200,000-item budget per flow pass |

The image decoder validates PNG/JPEG bytes and dimensions before embedding
them. Local images and linked CSS resolve against the HTML file's directory;
HTTP(S) resources use the existing URL resolver and final redirect response
metadata. Template content remains inert for style, image, and base-URL
loading.

## Pinned checks and rendering examples

There are **9 HTML fixture files, 1 PNG, and 1 CSS file** in `tests/render`.
The **16 rendering integration tests** cover 320, 640, and 900 px cards and
start-page layouts; exact box geometry and computed styles; line fragments;
text metrics; image dimensions and alt fallback; malformed CSS; clipping and
SVG XML structure; redirected CSS/image metadata; local linked CSS; min/max
constraints; empty/deep documents; and 20 repeated adversarial pages. Fixture source notes are in
[`tests/render/README.md`](../tests/render/README.md). Existing parser corpus
assertions still check 7,028 exact tokenizer sequences, 1,739 document trees,
206 fragment trees, eight script-on trees, and 7,028 tokenizer error lists.
The final locked suite passes **89 tests** with **3 intentionally ignored**
diagnostic inventory tests. `cargo fmt --all -- --check` and
`cargo clippy --all-targets -- -D warnings` pass on the final tree.

```sh
cargo run --locked --bin scarlite -- examples/welcome.html --output welcome.svg --width 900
cargo run --locked --bin scarlite -- tests/render/start_page.html --output start-page.svg --width 320
cargo run --locked --bin scarlite -- tests/render/images.html --output images.svg --width 400
cargo run --locked --bin scarlite -- https://example.com --output example.svg --width 900
```

Visual inspection of the rasterized SVGs confirms readable text and nested
red boxes in `welcome.html`, five rounded start-page tiles in two columns at
320 px and four in the first row at 900 px, PNG renditions at 80×40, 60×30,
and intrinsic 2×1 with missing-image alt text, and readable text from the HTTPS
example page. Fixture tests tie those visible box and image positions to the
scene geometry. The linked CSS/image redirect test asserts final response
metadata, one live image, and no template image request.

## Remaining gaps and next engine step

Phos has no JavaScript runtime or browser window. The renderer lacks full
selector/CSS syntax support, flex/grid/table layout, floats, positioning,
margin collapsing, complete inline baseline and fragment decoration rules,
font shaping/bidi, sophisticated line breaking, CSS backgrounds, and image
formats beyond PNG/JPEG. A useful next step is a shaped-text line breaker
with explicit font fallback and bidirectional runs, followed by WPT CSS
layout fixtures for the implemented block and inline subset.
