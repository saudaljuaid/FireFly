# Phos static text and positioned layout

The fresh fetched `main` was `3bf53f358a61c9557f467d65a7d35232e217ef6f`, the
previous static CSS/SVG milestone. Its baseline passed **89 tests, zero failed,
three intentionally ignored** diagnostics; formatting and Clippy passed before
changes. The pinned upstream parser sources and expected outputs are unchanged.
This document describes a bounded subset, not HTML, CSS, Unicode, font, or
browser conformance.
Two logo-only main updates arrived during work and were incorporated before
delivery; the delivery parent is `5f73036101b7827ca8a821580454a1a66cbb79db`.
They changed `assets/logo.webp` and no engine or parser source.

## Specifications used

The text model draws on these exact primary sections:

- [CSS Text 3 §1.4, Characters and Letters](https://www.w3.org/TR/css-text-3/#characters), [§1.5, Text Processing](https://www.w3.org/TR/css-text-3/#text-encoding), [§3, White Space and Wrapping](https://www.w3.org/TR/css-text-3/#white-space-property), [§4.1, White Space Processing](https://www.w3.org/TR/css-text-3/#white-space-rules), [§5, Line Breaking](https://www.w3.org/TR/css-text-3/#line-breaking), [§5.4, Overflow Wrapping](https://www.w3.org/TR/css-text-3/#overflow-wrap-property), and [§7.3, Shaping Across Element Boundaries](https://www.w3.org/TR/css-text-3/#boundary-shaping).
- [CSS Writing Modes 3 §2.1, Direction](https://www.w3.org/TR/css-writing-modes-3/#direction), [§2.2, Unicode Bidi](https://www.w3.org/TR/css-writing-modes-3/#unicode-bidi), and [§2.4, Bidi and Line Layout](https://www.w3.org/TR/css-writing-modes-3/#bidi-algo). Vertical writing is deferred.
- [CSS Inline Layout 3 §3.2, Baselines and Metrics](https://www.w3.org/TR/css-inline-3/#baseline-types), [§4.2.1, Baseline Source](https://www.w3.org/TR/css-inline-3/#baseline-source), [§5.1, Line Height](https://www.w3.org/TR/css-inline-3/#line-height-property), and [CSS 2.2 §10.8, Line Height](https://www.w3.org/TR/CSS22/visudet.html#line-height).
- [CSS Fonts 4 §5, Font Matching](https://www.w3.org/TR/css-fonts-4/#font-matching-algorithm), which informs the fixed-font fallback policy below; general CSS family matching is deferred.

Positioning and painting use [CSS 2.2 §9.4.2, Inline Formatting](https://www.w3.org/TR/CSS22/visuren.html#inline-formatting),
[§9.4.3, Relative Positioning](https://www.w3.org/TR/CSS22/visuren.html#relative-positioning),
[§9.6, Absolute Positioning](https://www.w3.org/TR/CSS22/visuren.html#absolute-positioning),
[§10.1, Containing Blocks](https://www.w3.org/TR/CSS22/visudet.html#containing-block-details),
[§9.9, Layered Presentation](https://www.w3.org/TR/CSS22/visuren.html#layers), and
[Appendix E.2, Painting Order](https://www.w3.org/TR/CSS22/zindex.html#painting-order).
The corresponding [CSS Positioned Layout 3 §2.1, Containing Blocks](https://www.w3.org/TR/css-position-3/#def-cb),
[§2.2, Stacking](https://www.w3.org/TR/css-position-3/#stacking),
[§3.1, Insets](https://www.w3.org/TR/css-position-3/#insets),
[§3.3, Relative Offsets](https://www.w3.org/TR/css-position-3/#relpos-insets),
[§3.5, Absolute Insets](https://www.w3.org/TR/css-position-3/#abspos-insets), and
[§4, Absolute Layout](https://www.w3.org/TR/css-position-3/#abspos-layout) were
also consulted; that 7 October 2025 publication is a working draft. These
sections describe more than the implemented matrix and deliberate differences.

## Supported CSS matrix

Unsupported declarations, values, selectors, and at-rules are ignored.

| Area/property | Supported subset | Deliberate limits |
| --- | --- | --- |
| Selectors | Tag, class, ID, compound, descendant | No pseudo selectors, other combinators, media queries |
| Cascade | UA defaults; linked/inline sheets in document order; style attributes; specificity, source order, inheritance, `!important` | No user sheets, layers, custom properties, `@import`, animations |
| Declaration syntax | Comments, escapes, quotes, nested functions/parentheses, semicolons in values, trailing `!important` | Bounded scanner, not general CSS Syntax; unterminated values recover at the enclosing rule |
| `display` | `block`, `inline`, `inline-block`, `none` | No flex, grid, table formatting, floats |
| Size | `width`, `height`, `min-width/height`, `max-width/height`, `box-sizing:content-box/border-box` | Approximate preferred width for non-replaced atomic/absolute boxes |
| Edges | One-to-four-value `margin`, `padding`, `border-width`; individual margin/padding/width sides; auto margins | No margin collapsing; side-specific border style/color absent |
| Borders | `border` width/style/color; `border-style:none/solid/dashed/dotted`; `border-color`, `border-radius` | Circular radii; asymmetric borders paint side rectangles; wrapped inline borders use sliced ends |
| Color | Hex RGB/RGBA, `rgb()`, `rgba()`, common names, transparent; alpha text/background/border | No full named-color table, Color 4 spaces, gradients, background images |
| Font | `font-size` resolving to 1…1,024 px; `font-weight:normal/400` or `bold/700/800/900`; `line-height:normal`, unitless 0.1…10, or length/percent resolving to 1…1,000,000 px | Fixed bundled family policy; other weights, italic, font shorthand, family matching, letter/word spacing deferred |
| White space | `normal`, `nowrap`, `pre`, `pre-wrap`, `pre-line`; `<br>` | Horizontal text; no `break-spaces` or tab-size control |
| Breaking | `overflow-wrap:normal/break-word/anywhere` | Default is deliberately `break-word`; no `word-break`, hyphenation, language tailoring |
| Alignment | `text-align:left/center/right/start/end` | No justification or vertical-align controls |
| Direction | Inherited `direction:ltr/rtl`; `unicode-bidi:normal/embed/isolate` | No override/plaintext values or vertical writing |
| Lists | `list-style-type:disc/decimal/none`; same single keyword in `list-style`; `<ol start>` | Outside marker at physical left; RTL marker placement, reversed/value attributes, custom counters/images deferred |
| Position | `static`, `relative`, `absolute`; `top/right/bottom/left`; one-to-four-value `inset`; `z-index:auto` or integer −32,768…32,767 | Fixed/sticky/logical insets/transforms deferred; local ancestor paint barriers |
| Clipping | `overflow:visible/hidden`; rounded padding-edge descendant clips | No scrolling or separate overflow axes |
| Images | PNG/JPEG `<img>`; intrinsic, HTML/CSS sizes, ratio preservation, alt fallback | No GIF/WebP/SVG decoding, srcset, CSS background images |

Lengths accept `px`, `em`, `rem`, `%`, and unitless zero. `em` uses the
computed element size; font-size `em/%` uses its parent, and `rem` uses the
root. Cascaded font size resolves before font-relative properties, so `1em`
padding and percentage line height do not depend on declaration order.
The root's own `font-size` uses the initial 16 px for `rem`; other root
properties and descendants use its computed font size.
Normal-flow horizontal percentages use the containing content width;
margin and padding percentages on all sides use that width. Normal-flow height
percentages require a definite containing height and otherwise remain auto.
Mixed block/inline descendants keep DOM order through anonymous block wrappers.

## Resolved text, whitespace, and line geometry

`Scene.runs` records the source DOM node and byte range, normalized paragraph
range, logical content, selected face, size/weight, direction, glyph
IDs/offsets/advances/clusters, ascent/descent/line gap, final x/baseline, line
index, visual order, and active inline boxes. `Scene.line_boxes` records bounds,
baseline, advance, paragraph direction, and the range of resolved runs.
Text primitives carry the same `ResolvedRun`. The painter neither reshapes nor
remeasures. A combining cluster crossing a DOM boundary belongs to its first
character's style/node; its primary source range stays in that node. Inline
edges inside a cluster are deferred until after it. List marker runs are
generated content and have empty source ranges.

Normalization precedes UAX #14 opportunities and UAX #9 analysis. Extended
grapheme boundaries constrain both ordinary and emergency breaks. Inline
boundaries do not add a break, and collapsible spaces collapse across adjacent
elements. Bidi analysis keeps DOM content logical, reorders resolved pieces
visually per line, and resets preserved trailing whitespace to the paragraph
level (UAX #9 L1). Inline embed/isolate values add analysis controls rather than
visible characters. Punctuation, parentheses, numbers, and mixed Arabic/Hebrew
with Latin are covered by fixtures, not a claim of full UAX #9 integration.

The locked implementations are `unicode-linebreak` 0.1.5 (Unicode 15.0),
`unicode-bidi` 0.3.18 (Unicode 16.0), and `unicode-segmentation` 1.13.3
(Unicode 17.0). These independently maintained table versions are intentionally
reported separately; the engine does not claim a single full Unicode version.
Linebreak uses Apache-2.0; bidi/segmentation use MIT/Apache-2.0. Unicode data
notices remain in dependency packages; no copied tables were added to this repo.

| White-space value | Spaces/tabs | Newlines | Wrapping |
| --- | --- | --- | --- |
| `normal` | Collapse ASCII whitespace, trim line edges | Collapse | Unicode opportunities plus configured emergency behavior |
| `nowrap` | Collapse and trim | Collapse | No ordinary/emergency wrap, including adjacent atomic boxes |
| `pre` | Preserve; eight-space tab stops | Preserve | No wrap |
| `pre-wrap` | Preserve; eight-space tab stops | Preserve | Unicode opportunities plus configured emergency behavior |
| `pre-line` | Collapse spaces/tabs, trim edges | Preserve | Unicode opportunities plus configured emergency behavior |

Consecutive preserved newlines and `<br>` create empty line boxes with the
inherited text strut; a trailing break does not invent one extra line. Empty
undecorated inline/text and collapsible-whitespace-only content add no height.
NBSP, narrow NBSP, and word joiner suppress emergency breaks under the default
`break-word`; `anywhere` may break these groups at grapheme boundaries. Under
`normal`, an overlong indivisible group overflows. A width smaller than one
glyph still places one complete cluster before advancing to the next line.
`pre-wrap` allocates preserved trailing-space advances rather than implementing
CSS hanging-space rules; space-only wrapping can consequently differ from a
browser. Tab stops use eight advances of a space in the selected face.

Inherited text properties are color, font size, regular/bold weight, line
height, alignment, white space, direction, overflow wrap, and list style.
`unicode-bidi`, position, offsets, and z-index are not inherited. Normal and
unitless line height inherit a multiplier; lengths and percentages inherit
computed pixels. Normal uses the deliberate 1.2 multiplier. Font metrics and
half-leading establish each participating baseline contribution, and mixed
font sizes/faces contribute the maximum ascent and descent. Images align their
bottom edge to the alphabetic baseline. Inline-blocks use the final internal
line baseline when overflow is visible, otherwise their bottom edge; empty
atomic-only lines use their actual atomic height.

Inline top/bottom padding and borders paint beyond the line-height contribution.
Side padding/borders consume advance at logical ends. Wrapped backgrounds form
separate visual line fragments; continuation fragments omit side borders and
end radii. The document keeps anonymous blocks, text alignment, and normal-flow
space when relative descendants move.

## Fonts, shaping, and SVG output

Regular/bold DejaVu Sans are the existing unmodified fonts. Tested Latin accents,
decomposed marks, Arabic contextual forms, Hebrew, and ligatures use HarfRust
0.13.3, the Rust HarfBuzz shaper. A fixed five-codepoint context preserves
joining across compatible color-only DOM boundaries. Font size, weight,
bidi-level, atomic content, and nonzero decoration boundaries isolate context;
cross-run kerning/ligatures and full CSS boundary shaping remain gaps.
HarfRust was selected because its maintained Rust HarfBuzz implementation
provides the glyph IDs, cluster mappings, positioning, and OpenType contextual
forms needed by both line layout and outline SVG. Its MIT license permits this
use. The previous sum of character advances could not represent these shaping
results. Fixed font assets and deduplicated outlines avoid an installed-font
dependency; the CJK derivative is 2.8 MB instead of its 16.4 MB full source.

Each face retains at most 64 FIFO shaping plans (192 across all three faces).
HarfRust's complete `ShapePlanKey` checks script, direction, language, features,
and variation coordinates. Current fonts have no configured custom language,
features, or variations, and plans are size-independent. Arc references keep
an active plan valid after eviction; shaping occurs outside the cache lock.
Cached/uncached glyph geometry and cold/warm output equality are tested.
Each face also retains 128 font-derived ASCII coverage booleans, 384 bytes
across all three faces. This replaces repeated cmap lookups for common prose;
all 128 entries are tested against each face's cmap. Non-ASCII selection keeps
the same cmap path.

The renamed **2,826,132-byte** `PhosCjk-Regular.otf` derivative of Noto Sans
CJK SC 2.004 supplies **10,805 Unicode mappings**: complete GB2312 and JIS
X0208 repertoires, kana, and punctuation. Its repertoire is independent of
fixtures. The [font source notes](../assets/fonts/README.md) pin the upstream
commit, source/derivative SHA-256 hashes, exact reproduction command, and
coverage. DejaVu's [Bitstream Vera notices](../assets/fonts/LICENSE-dejavu.txt)
and the derivative's [SIL OFL 1.1](../assets/fonts/LICENSE-noto-cjk.txt) are
preserved. The reduced face is renamed in both OpenType and CFF metadata.
Rendered outline SVG uses the OFL document exception; redistributed font assets
retain the license. Glyphs use Simplified Chinese forms, without regional
alternates, synthetic CJK bold, Korean Hangul, uncommon Han, or emoji coverage.

Fallback selects the requested DejaVu face, regular DejaVu if bold coverage is
absent, then CJK, then visible U+FFFD replacement. Selection occurs at grapheme
boundaries and keeps missing counts in the resolved run. CJK has one regular
face; requested bold remains metadata. A damaged font parse supplies finite
0.6-em replacement boxes per grapheme. Missing optional fonts/glyphs never
consult installed system fonts or panic.

SVG paints deduplicated glyph outlines through positioned `<use>` elements.
Each logical run has a `<g data-phos-text>` with face/direction/advance
metadata and an escaped accessible `<title>`. Visible `<text>` and embedded
font binaries have deliberately been replaced. Viewers do not rerun shaping,
bidi, fallback, or `textLength`. Glyph geometry is deterministic within SVG
path support; antialiasing can differ. Outline SVG has limited ordinary text
selection/search and text-node consumer support. Images remain embedded,
resource IDs are unique, and XML text/attributes are escaped.

## Positioning and paint order

Relative positioning translates the completed box and descendants, including
runs, line geometry, images, and clips, without changing the reserved flow
space. If both horizontal insets resolve, the containing block's start side
wins: left in LTR, right in RTL. Top wins over bottom. Relative vertical
percentages with indefinite parent height behave as auto.

Absolute descendants leave normal flow. A bounded post-flow DOM-order pass
resolves the nearest positioned block/inline-block ancestor's complete padding
rectangle. Its final auto flow height is usable in this deliberate subset;
general browser percentage-height behavior is more nuanced. Positioned inline
fragments do not establish containing rectangles. Without a supported ancestor,
the initial rectangle is x=0, y=0, viewport width, and indefinite height.
Initial vertical percentage offsets/heights remain unresolved. The CLI has no
viewport height or fixed-position semantics.

Absolute sizes and physical inset percentages use the complete padding width or
height before opposing insets reduce usable space. Every percentage
margin/padding uses the containing width. Explicit sizes, min/max constraints,
box sizing, margins/padding/borders, opposing-inset stretch, and auto margins
between resolved opposing insets are supported. One-sided/all-auto width uses
a bounded preferred-width shrink approximation; auto height uses content.
All-auto position uses the nearest supported ancestor's content origin, a
deliberate approximation to CSS static position.

Each principal block/inline-block is a local paint barrier. Its own
background/border and clip opening precede descendants, then negative z-index
groups in increasing order, ordinary in-flow content in document order,
positioned auto/zero groups in document order, and positive z-index groups in
increasing order. Equal values use DOM order; clip closing follows the group.
Positioned non-auto z-index expresses the supported stacking intent, but all
local barriers retain descendants. Full Appendix E promotion through
non-stacking ancestors is deferred. Inline fragments paint before line ink;
inline z-index has no independent principal group. Intact groups preserve alpha,
rounded corners, and ancestor clipping under sorting.
Before a negative positioned principal child, the inline ancestors' recorded
background/border fragments paint once, outer ancestor before inner, inside the
nearest principal group's active clip. Ordinary painting then skips those
already emitted fragments. This preserves ancestor decorations before negative
descendants without creating independent fragmented-inline stacking contexts
or promoting descendants beyond the local principal barriers.

## Bounds and failures

| Input/resource/work | Bound |
| --- | ---: |
| HTML bytes, including decoded byte input | 16 MiB |
| Open HTML elements / token reprocessing | 256 / 32 steps |
| Viewport width / scene coordinates | 1–16,384 CSS px / finite ±1,000,000 |
| CSS declarations per scan / name / value | 8,192 / 128 bytes / 65,536 bytes |
| Linked CSS requests / individual source / accumulated styles | 16 / 2 MiB / 4 MiB including separators |
| Image requests / compressed bytes each | 16 / 4 MiB |
| Image dimensions / decoded pixels / decoder allocation | 16,384 per axis / 16 million / 64 MiB |
| Redirects | 5; HTTPS-to-HTTP downgrade refused |
| Shared text/inline work | 200,000 normalized scalars/edge/atomic operations across nested flow |
| Layout traversal depth | 256 internal traversal steps; excess reports truncation |
| Flattened flow items | 200,000 including reserved closures for open inline boxes |
| Individual shape source / emitted glyphs | 65,536 bytes / 65,536 glyphs |
| Individual resolved run advance | 250,000 CSS px |
| Retained shaping plans | 64 FIFO per face; 192 across the three bundled faces |
| Cached ASCII coverage | 128 booleans per face; 384 bytes across the three faces |
| Glyph work | 200,000 per resolve-context invocation; 200,000 painted placements globally |
| Box, primitive, run, line lists | Independently 200,000 items |
| Glyph outline / all definitions / SVG output | 65,536 bytes / 16 MiB / 128 MiB |
| Logical SVG title | 65,536 bytes |

Only the three trusted bundled font faces are decoded; arbitrary external font
input is unsupported. The emitted-glyph limit does not claim a separate cap on
HarfRust's internal shaping-buffer allocation.

The checked CSS append helper now bounds inline styles in direct string/byte,
local-file, and remote rendering, including separator bytes. Oversized inline
sheets are skipped while later valid rules and page content survive. A remote
linked sheet rejected by the combined cap ends the remaining stylesheet scan,
preserving the previous loader behavior; page content still renders. The raw
`Document.stylesheets()` inspection utility is unchanged; rendering entry
points use bounded extraction. Template style/image/base URL content stays inert.
Optional CSS/image failures preserve renderable page content. Local file and
HTTP(S) loading retain the existing resolver/network stack, redirect final
metadata, PNG/JPEG validation, alt text, and aspect ratio behavior.

Normalization, opportunities, bidi, and face selection use bounded passes.
Emergency splitting reuses shaped cluster advances, then shapes final chunks;
it does not invoke a shaper for every candidate character. Graphemes are never
partially accepted at the work boundary. Source chunks use cached face maximum
advances and requested size. A shaped run exceeding its advance bound is
bisected only at grapheme boundaries (at most 16 levels for a 64 KiB chunk).
A single oversized cluster retains logical content but paints one replacement
cluster. Measured advances are not silently saturated. The line builder stops
before position saturation, retains a deterministic finite prefix, and reports
`Scene.truncated`; SVG clipping remains balanced.
Depth and flow-item exhaustion also report truncation. Flow flattening reserves
an end item for each open inline box, so an accepted prefix retains balanced
inline fragment state at the item boundary.
SVG root metadata exposes truncation, and `scarlite` prints a bounded-prefix
warning when rendering or painting exhausts its budget. Successful optional
resource loading does not bypass these scene/output limits.
Intrinsic text widths are cached across nested atomic layout. Empty inline-edge
line state is retained directly rather than rescanning every prior edge.

`Scene.height` is the maximum of normal-flow height and painted box extent,
clamped to 1…1,000,000 px. Extent currently includes box geometry even when an
ancestor clips it; an oversized clipped descendant can therefore add blank
SVG page space. Clipping changes visible painting, not this extent policy.

## Fixtures and verification

There are **21 HTML source files, one PNG, and one CSS** in `tests/render`.
The twelve new sources are original Apache-2.0 fixtures. Exact provenance and
the retained Scarlite start-page source pin are in
[fixture notes](../tests/render/README.md); the UI repository is unchanged.
The **71 rendering integration checks** comprise the original **16 rendering
tests**, **17 text-layout**, **21 positioning**, **9 text-boundary**, and
**8 line-detail tests**.

Fixtures exercise all five whitespace modes; ASCII punctuation/hyphens/URLs;
Latin accents/decomposed marks; CJK; NBSP/narrow NBSP; cross-element whitespace
and combining clusters; mixed Arabic/Hebrew/Latin/numbers; regular/bold/fallback
faces; line baselines and sliced inline fragments; images/inline-blocks; lists;
relative/absolute offsets, containing rectangles, z-index/alpha/clipping; an
article, navigation/cards, and expanded many-span/long-token/malformed inputs.
Assertions cover exact metrics, source mapping, visual order, line counts,
baselines, dimensions, scene order, unique valid SVG IDs, and bounded work,
alongside visual review. Screenshots are review artifacts, not the sole oracle.

The unchanged corpus checks **7,028 current and legacy tokenizer sequences**,
**1,739 document trees, 206 fragment trees, eight script-on trees**, and **7,028
tokenizer error lists**. Four surrogate inputs remain unrepresentable. The
three ignored diagnostics are `inventory_upstream_tokenizer`,
`inventory_upstream_document_trees`, and `inventory_upstream_fragments`; the
independent WPT diagnostic gaps remain in [engine status](ENGINE_STATUS.md).
The final local `cargo test --locked` passes **167 tests, zero failed, three
intentionally ignored**. The exact groups are 47 library, 10 byte-input,
8 line-detail, 6 parse-error, 21 positioning, 16 rendering, 9 text-boundary,
17 text-layout, 22 tree-construction, 2 upstream-error, 2 upstream-tokenizer,
and 7 upstream-tree tests; binary/doc tests contain zero cases. All focused
rendering/stress checks are included in this locked suite. Formatting,
`cargo clippy --all-targets -- -D warnings`, and `git diff --check` pass.
The [GitHub workflow](../.github/workflows/ci.yml) runs those same formatting,
Clippy, and locked-suite commands; the delivery report links its completed run
and records verified CI results.

The local checks use Rust under WSL Ubuntu; Windows formatting also succeeds.
Native MSVC compilation on this machine lacks `link.exe`, an environment
limitation rather than a source test failure. CI uses its configured Linux
environment.

## CLI examples, visual review, and performance

SVG/PNG review artifacts are saved outside the repository. Reproduce source
SVGs with the normal `scarlite` command and inspect them in an SVG-capable
browser; the delivery report links representative narrow/wide previews.

```sh
cargo run --locked --bin scarlite -- examples/welcome.html --output welcome.svg --width 900
cargo run --locked --bin scarlite -- tests/render/start_page.html --output start-320.svg --width 320
cargo run --locked --bin scarlite -- tests/render/start_page.html --output start-640.svg --width 640
cargo run --locked --bin scarlite -- tests/render/start_page.html --output start-900.svg --width 900
cargo run --locked --bin scarlite -- tests/render/article.html --output article.svg --width 320
cargo run --locked --bin scarlite -- tests/render/bidi.html --output bidi.svg --width 320
cargo run --locked --bin scarlite -- tests/render/unicode.html --output unicode.svg --width 640
cargo run --locked --bin scarlite -- tests/render/position_clip.html --output clip.svg --width 900
cargo run --locked --bin scarlite -- https://example.com --output example.svg --width 900
```

The historical baseline was inspected through actual Chrome 154.0.8037.92.
It began RTL paragraphs from the left, misplaced mixed RTL content, and relied
on installed fonts for CJK. The final **23 browser-rendered SVG/PNG pairs** cover
welcome/start/article/bidi/inline/navigation/stacking/clipping at 320/900 px,
start/article also at 640 px, Unicode at 60/640 px, HTTPS at 320/900 px, and the
clipped negative child through wrapped inline decorations. Every inspected SVG
has zero browser errors, duplicate IDs, non-finite text boxes, and visible
`<text>` nodes. The outlined glyph groups preserve selected faces/direction.

Visual inspection confirms attached decomposed accents, visible CJK outlines,
usable Arabic/Hebrew visual order, shared mixed-size/image baselines, sliced
wrapped decorations, readable narrow article/navigation content, clear NOTE
and NEW overlays, and the tested stacking/rounded clips. The start-page canary
keeps five tiles at 320/640/900 px and heights 411/303/303 px. The article is
2553/1418/1278 px tall at those widths. The unicode-640 preview contains 48
bundled CJK run groups. The article's
original absolute NOTE label needed reserved top padding under DejaVu; the
font-matched source browser reproduced the collision before this original
fixture was corrected. Welcome's original fixed 720 px main intentionally
overflows a narrow viewport. HTTPS source is live rather than a pinned oracle.

Local debug CLI timing alternates four historical/final process pairs per
identical source, median of each engine's last three, at 640 px. The article source is 2,982 bytes; stress
sources contain 4,000 spans (74,077 bytes) and a 50,000-character token
(50,077 bytes). Baseline worktree is the historical commit above. These are
local records, not portable benchmarks.

| Case | Baseline median | Final median | Baseline SVG bytes | Final SVG bytes |
| --- | ---: | ---: | ---: | ---: |
| Article | 0.506598 s | 0.341179 s | 2,031,214 | 334,128 |
| 4,000 spans | 0.698914 s | 0.498184 s | 3,700,767 | 5,740,649 |
| 50,000-character token | 0.422889 s | 0.479856 s | 2,208,919 | 5,673,227 |

An intermediate long-token implementation regressed because it shaped every
grapheme during emergency sizing. It was replaced with shared shaped-cluster
advances. Profiling then found repeated OpenType plan compilation and ASCII cmap
lookups; bounded plan/coverage caches and ordered-vector cluster reduction
removed those costs without changing SVG bytes. In this final local window the
article and span cases take 0.67/0.71 times the historical median; the long token
takes 1.13 times as long. Its richer glyph/run metadata makes SVG 2.57 times
larger, while the article is substantially smaller. Process/host variation is
material, so these figures establish investigated costs rather than a portable
speed guarantee. Intermediate measurements remain external review records.

Nested intrinsic-width caching was separately checked at 64/128 atomic levels:
an intermediate 4.637/8.362 seconds became 0.438/0.494 seconds, with the 64-level
SVG unchanged at 1,103,699 bytes. The 128-level case explicitly reaches the
internal depth bound and emits a 205-byte truncated result; this is failure
reporting, not complete content rendering.

## Remaining gaps and next step

No flex/grid/table formatting, margin collapsing, floats, transforms,
multicolumn, sticky/fixed positioning, vertical writing, custom/variable fonts,
general CSS font matching, emoji, general Korean/Indic/Thai coverage,
hyphenation, tailored line breaking, justification, full cross-element shaping,
fragment containing blocks, or full CSS painting-context promotion is claimed.
Outline SVG text selection/search is limited; explicit tight line heights or
position offsets can still intentionally overlap. Parser diagnostic gaps remain
independently documented. There is no JS runtime, live DOM/event loop, desktop
window, browser chrome, crawler, or separate UI application.

The next concrete engine step is to pin WPT CSS cases for the implemented inline
and positioning subset, then implement fragment containing blocks and promotion
of positioned descendants through non-stacking ancestors while preserving clips
and the same resolved runs. Complete cross-element shaping can follow those
geometry checks.
