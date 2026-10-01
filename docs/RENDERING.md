# Phos static responsive layout

Phos is Inkbird's headless bounded backend for the native Scarlite-UI host.
This milestone adds a shared intrinsic-sizing foundation, horizontal Flexbox
and Grid, width-based
responsive CSS, viewport lengths and calculations, linear gradients and outer
shadows. It does not add an application, browser window, live DOM or runtime.
This is a substantial supported subset, not HTML/CSS/browser conformance.

Fresh inventory fetched `main` at
`a89508a053c5e4260ac2ee60927690718249cf58`; there were no intervening commits.
Baseline local checks and the completed GitHub job passed **167 tests, zero
failed, three intentionally ignored** diagnostics. Parser corpora, network/URL
implementation, resources, dependencies, bundled fonts and Scarlite-UI remain
unchanged. Detailed contracts are [Flexbox](FLEXBOX.md), [Grid](GRID.md),
[responsive CSS](RESPONSIVE_CSS.md), [effects](EFFECTS.md) and
[browser/performance evidence](BROWSER_COMPARISON.md).

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
| Selectors | Tag, class, ID, compound, descendant | No pseudo selectors or other combinators |
| Cascade | UA defaults; linked/inline sheets in document order; style attributes; specificity, source order, inheritance, `!important` | No user sheets, layers, custom properties, `@import`, animations |
| Declaration syntax | Comments, escapes, quotes, nested functions/parentheses, semicolons in values, trailing `!important` | Bounded scanner, not general CSS Syntax; unterminated values recover at the enclosing rule |
| `display` | `block`, `inline`, `inline-block`, `flex`, `inline-flex`, `grid`, `inline-grid`, `none` | No table formatting or floats |
| Size | `width`, `height`, `min-width/height`, `max-width/height`, `box-sizing:content-box/border-box`; horizontal `min-content`/`max-content` | Intrinsic height keywords fall back to automatic/natural behavior; no fit-content() |
| Edges | One-to-four-value `margin`, `padding`, `border-width`; individual margin/padding/width sides; auto margins | No margin collapsing; side-specific border style/color absent |
| Borders | `border` width/style/color; `border-style:none/solid/dashed/dotted`; `border-color`, `border-radius` | Circular radii; asymmetric borders paint side rectangles; wrapped inline borders use sliced ends |
| Color | Hex RGB/RGBA, `rgb()`, `rgba()`, common names, transparent; alpha text/background/border | No full named-color table, Color 4 spaces or external CSS background images |
| Font | `font-size` resolving to 1…1,024 px; `font-weight:normal/400` or `bold/700/800/900`; `line-height:normal`, unitless 0.1…10, or length/percent resolving to 1…1,000,000 px | Fixed bundled family policy; other weights, italic, font shorthand, family matching, letter/word spacing deferred |
| White space | `normal`, `nowrap`, `pre`, `pre-wrap`, `pre-line`; `<br>` | Horizontal text; no `break-spaces` or tab-size control |
| Breaking | `overflow-wrap:normal/break-word/anywhere` | Default is deliberately `break-word`; no `word-break`, hyphenation, language tailoring |
| Alignment | `text-align:left/center/right/start/end` | No justification or vertical-align controls |
| Direction | Inherited `direction:ltr/rtl`; `unicode-bidi:normal/embed/isolate` | No override/plaintext values or vertical writing |
| Lists | `list-style-type:disc/decimal/none`; same single keyword in `list-style`; `<ol start>` | Outside marker at physical left; RTL marker placement, reversed/value attributes, custom counters/images deferred |
| Position | `static`, `relative`, `absolute`; `top/right/bottom/left`; one-to-four-value `inset`; `z-index:auto` or integer −32,768…32,767 | Fixed/sticky/logical insets/transforms deferred; local ancestor paint barriers |
| Clipping | `overflow:visible/hidden`; rounded padding-edge descendant clips | No scrolling or separate overflow axes |
| Images | PNG/JPEG `<img>`; intrinsic, HTML/CSS sizes, ratio preservation, alt fallback | No GIF/WebP/SVG decoding, srcset, CSS background images |
| Flexbox | Actual base/hypothetical sizing, scaled shrink/grow with min/max freezing, lines, gaps, reverse/RTL, auto margins, cross stretching and first baselines | See exact property matrix and intrinsic simplifications in [FLEXBOX.md](FLEXBOX.md) |
| Grid | Explicit/sparse automatic placement, implicit tracks, fixed/percentage/intrinsic/fr/minmax/integer-repeat sizing, spans and alignment | No auto-fit/fill, dense, named areas, subgrid or complete cyclic dependency reruns; see [GRID.md](GRID.md) |
| Responsive | Bounded width features/ranges, and/comma alternatives, nested media, style/link media attributes | Static viewport environment; no container queries or other media features |
| Values | px/em/rem/%/vw; vh/vmin/vmax with explicit height; typed bounded calc arithmetic | Unresolved percentages remain unresolved; see [RESPONSIVE_CSS.md](RESPONSIVE_CSS.md) |
| Effects | One linear gradient, up to four outer shadows; circular rounded paths, real SVG Gaussian blur and knockout masks | No inset/repeating/radial effects; bounded interpolation and surface policies in [EFFECTS.md](EFFECTS.md) |

Lengths accept `px`, `em`, `rem`, `%`, `vw`, and unitless zero, plus bounded
`calc()`. Height-based viewport units require the explicit viewport height. `em` uses the
computed element size; font-size `em/%` uses its parent, and `rem` uses the
root. Cascaded font size resolves before font-relative properties, so `1em`
padding and percentage line height do not depend on declaration order.
The root's own `font-size` uses the initial 16 px for `rem`; other root
properties and descendants use its computed font size.
Normal-flow horizontal percentages use the containing content width;
margin and padding percentages on all sides use that width. Normal-flow height
percentages require a definite containing height and otherwise remain auto.
Mixed block/inline descendants keep DOM order through anonymous block wrappers.

## Shared sizing and formatting contexts

`src/sizing.rs` separates definite available space, indefinite space,
min-content/max-content queries, content dimensions and outer contributions.
`AxisSpace` additionally separates a constrained used height from a definite
percentage basis: an auto-height Flexbox with `min-height:100px` can distribute
space across 100px while a `50%` basis still falls back to content. Explicit
height provides a percentage basis. Grid's final areas provide a basis for
an item's percentage height; stretched and resolved preferred heights provide
the descendant basis, while unstretched natural heights retain unresolved
descendant percentages. Unresolved percentage tracks/gaps follow its
documented intrinsic policy.

`IntrinsicCache` lives for one immutable document/computed-style/image pass.
It measures normalized logical paragraphs, Unicode opportunities, selected
faces and shaped advances through the established paragraph/text pipeline.
Min-content uses unbreakable groups; NBSP survives. `anywhere` uses shaped
cluster-derived grapheme opportunities; `break-word` emergency breaks are not
intrinsic opportunities. Max-content omits soft wrapping but respects forced
lines. Tabs use the same space advance. Inline decorations and atomic content
contribute; block children take maxima. Images include intrinsic ratio,
HTML hints, constrained preferred-axis transfer and the both-auto min/max
ratio table from [CSS 2.2 §10.3.2](https://www.w3.org/TR/CSS22/visudet.html#inline-replaced-width)
and [§10.4](https://www.w3.org/TR/CSS22/visudet.html#min-max-widths).
Opposing constraints can override the ratio. Valid authored `auto`
suppresses the corresponding HTML dimension hint. Empty/failed images retain
alt fallback. Hidden/out-of-flow/inert content does not contribute.

Contributions add unresolved-free padding, borders and non-auto margins after
preferred/min/max constraints. Numeric border-box sizes subtract insets;
intrinsic keywords always select content dimensions. Minimum wins when min
and max conflict. Percentages/calc expressions retain their containing-size
dependency, including zero percentages; they are not resolved from viewport
width during intrinsic measurement. Cyclic percentage edges are omitted in
intrinsic contributions and resolve at final available width. Atomic and
one-sided absolute auto widths use `min(max(min-content,available),max-content)`.

Flexbox intrinsic widths conservatively combine item contributions and definite
bases; its ideal intrinsic flex-fraction algorithm is deferred. Grid uses the
same placement, intrinsic track and automatic-minimum helpers for measurement
and final layout. Column natural heights and Grid rows are width-dependent dry
measurements, distinct from intrinsic width contributions. These dry passes do
not append boxes, glyph primitives, clips, paint groups or SVG resources, or
consume the final output budget. A size-keyed cache includes node, available
width/height, forced content sizes and whether only children are measured.
It is not reused across documents or style/image changes. Definite fixed sizes
avoid unnecessary intrinsic or cross measurements. Bounded global counters and
reservations prevent suspended ancestors doing unpaid work after descendants
exhaust the numeric budget.

Final Flex/Grid items all pass through the common box and paragraph layout.
They retain image/alt policy, baseline construction, min/max/box sizing,
relative offsets, padded absolute anchors, local paint groups, rounded clips
and scene budgets. Direct adjacent text across comments forms one anonymous
item; whitespace-only runs are omitted. `order` creates stable formatting and
paint order, independently of physical reverse/RTL placement and logical DOM
text. Static Flex/Grid item z-index participates in local paint phases.
Inline-flex/inline-grid are atomic and expose their first supported baseline;
ordinary inline-block retains its previous last-flow/bottom-when-clipped policy.
Clipped Flex/Grid items retain a real first text baseline; empty/image items
synthesize their border-bottom baseline.

This foundation uses [CSS Sizing §2 terminology](https://www.w3.org/TR/css-sizing-3/#terms),
[§3.2 sizing values](https://www.w3.org/TR/css-sizing-3/#sizing-values),
[§3.3 box sizing](https://www.w3.org/TR/css-sizing-3/#box-sizing),
[§5.1 intrinsic sizes](https://www.w3.org/TR/css-sizing-3/#intrinsic-sizes),
[§5.2 intrinsic contributions](https://www.w3.org/TR/css-sizing-3/#intrinsic-contribution),
[CSS Box Alignment §5 content distribution](https://www.w3.org/TR/css-align-3/#content-distribution),
[§6 self alignment](https://www.w3.org/TR/css-align-3/#self-alignment) and
[§8 gaps](https://www.w3.org/TR/css-align-3/#gaps).
The linked subsystem contracts cite the exact Flexbox/Grid/Values/Media/Images
and Backgrounds sections used; their additional specification behavior is not
claimed by this subset.

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
the initial rectangle is x=0, y=0, viewport width and the optional explicit
viewport height. Width-only calls leave initial vertical percentages unresolved.
`--height` supplies that environment; fixed-position semantics remain deferred.

Absolute sizes and physical inset percentages use the complete padding width or
height before opposing insets reduce usable space. Every percentage
margin/padding uses the containing width. Explicit sizes, min/max constraints,
box sizing, margins/padding/borders, opposing-inset stretch, and auto margins
between resolved opposing insets are supported. One-sided/all-auto width uses
shared min-content/max-content shrink-to-fit; auto height uses content.
All-auto position uses the nearest supported ancestor's content origin, a
deliberate approximation to CSS static position.

Each principal block/atomic formatting container is a local paint barrier. Its own
background/border and clip opening precede descendants, then negative z-index
groups in increasing order, ordinary in-flow content in formatting order,
positioned auto/zero groups in document order, and positive z-index groups in
increasing order. Equal values use stable order-modified item order where applicable, otherwise
DOM order; clip closing follows the group. Reverse direction only changes placement.
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
| Supplied viewport width/height / scene coordinates | 1–16,384 CSS px / finite ±1,000,000 |
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
| Stylesheet rules / combined declarations | 8,192 / 65,536 |
| Selector list / descendant parts / classes per compound | 128 / 32 / 32 |
| Cascade visits and inspected declaration/attribute bytes | 8,000,000 globally; accepted author prefix plus inherited/UA defaults, status/truncation reported |
| Media nesting / alternatives / predicates | 8 / 16 / 16; evaluated once per viewport |
| calc bytes / nesting / tokens | 4,096 / 32 / 1,024; typed finite arithmetic |
| Intrinsic scalar/edge/structural work / numeric visits | 400,000 / 8,000,000 globally |
| Dry measurement operations / cached size entries | 600,000 / 16,384 globally |
| Formatting items / anonymous text members | 4,096 per container / 4,096 per anonymous run |
| Shared layout numeric visits | 8,000,000 globally, including reserved cross work before recursion |
| Flex numeric visits / freeze iterations | 4,000,000 per plan / at most n+1 |
| Grid tracks / span / repeat expansion | 256 per axis / 256 / 256 |
| Grid occupancy / placement probes / numeric visits | Fixed 8 KiB / 262,144 / 4,000,000 per phase |
| Gradient stops / outer shadows | 16 / 4 per box |
| Effect values / shadow offset or spread / blur | 4,096 bytes / ±4,096 px / 0–256 px |
| Effect definitions | 4,096, sharing the existing 16 MiB definition budget |
| Shadow surface area | 16,777,216 CSS px² per shadow; 67,108,864 per SVG |
| Premultiplied alpha interpolation | Depth 8; at most 256 leaves per adjacent authored stop interval; channel error target 1/1,024 |

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
Intrinsic contributions are cached across nested formatting and atomic layout. Empty inline-edge
line state is retained directly rather than rescanning every prior edge.

`Scene.height` is the maximum of normal-flow height and painted box extent,
clamped to 1…1,000,000 px. Extent currently includes box geometry even when an
ancestor clips it; an oversized clipped descendant can therefore add blank
SVG page space. Clipping changes visible box painting, not this retained extent policy. Outer
shadow bottom extents are added conservatively and respect active ancestor
clip bottoms. Effects do not reserve flow space or expand viewport width.

## Fixtures and verification

There are **28 HTML sources, four PNGs and one CSS** in `tests/render`.
The [fixture inventory](../tests/render/README.md) records the six restrained
layout compositions, effects probe, existing canary and original provenance.
No Scarlite-UI source was changed. New tiny gallery images are original PNGs,
reproducible with `tools/generate_layout_images.py`; no screenshot or upstream
expected output was adapted. No dependency/font policy was added or changed.
The existing dependency and font license records remain applicable.

Geometry and work tests cover intrinsic contributions/constraints, definite and
indefinite percentages, Flexbox base/freeze/line/cross results, Grid placement,
track spans/fractions, responsive computed styles and explicit transitions,
Unicode/bidi/baselines, image ratios, padded overlays, static item z-index,
source/paint order, clipping, unique resource IDs, finite bounds and truncation.
Screenshots supplement those oracles. The former invalid `calc(10px)` fixture
assertion now uses the independently invalid `calc(10px + red)` because the
former expression is supported and separately tested. Parser expectations
and corpus files are unchanged.

The final locked suite count and group breakdown are recorded in
[ENGINE_STATUS.md](ENGINE_STATUS.md). Baseline was 167/0/3; the three ignored
inventory diagnostics remain `inventory_upstream_tokenizer`,
`inventory_upstream_document_trees` and `inventory_upstream_fragments`.
Unchanged parser checks include **7,028 current and legacy tokenizer sequences**,
**1,739 document trees, 206 fragment trees, eight script-on trees**, and **7,028
error lists**. Four surrogate inputs remain unrepresentable. The independent
WPT diagnostic gaps remain in engine status.

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
git diff --check
```

All focused checks are included in the locked suite. Local tests/Clippy use
Rust 1.98.1 under WSL Linux; native MSVC is unavailable because this machine
lacks `link.exe` and Application Control blocks build helpers. No limits or
source requirements were changed to bypass that host limitation. GitHub's
[workflow](../.github/workflows/ci.yml) runs the same fmt, all-target Clippy and
locked commands; completed logs must be verified before delivery.

## CLI, actual browser comparisons and performance

SVGs and actual Chrome raster previews are stored outside the repository.
[BROWSER_COMPARISON.md](BROWSER_COMPARISON.md) records font matching, narrow/
medium/wide layouts, geometry/raster comparisons, honest remaining differences,
HTTPS behavior, and timing measurements with investigated regressions.

```sh
cargo run --locked --bin scarlite -- examples/welcome.html --output welcome.svg --width 900
cargo run --locked --bin scarlite -- tests/render/start_page.html --output start-320.svg --width 320
cargo run --locked --bin scarlite -- tests/render/start_page.html --output start-640.svg --width 640
cargo run --locked --bin scarlite -- tests/render/start_page.html --output start-900.svg --width 900
cargo run --locked --bin scarlite -- tests/render/layout-navigation.html --output navigation.svg --width 320
cargo run --locked --bin scarlite -- tests/render/layout-documentation.html --output documentation.svg --width 900
cargo run --locked --bin scarlite -- tests/render/layout-dashboard.html --output dashboard.svg --width 900
cargo run --locked --bin scarlite -- tests/render/effects.html --output effects.svg --width 640
cargo run --locked --bin scarlite -- https://example.com --output example.svg --width 900 --height 600
```

The seven original stress sources/timing cases are reproducible with the
standard-library-only `tools/measure_layout.py`, writing outside the checkout.
Elapsed times are engineering observations for this host, not portable speed
guarantees; the historical engine did not execute Flex/Grid algorithms.

## Remaining gaps and next concrete milestone

The subsystem contracts document ideal Flexbox intrinsic fractions, full
replaced-element automatic minima, Grid cyclic dependency reruns, auto-fit/fill,
advanced alignment/placement, inset shadows and broader background syntax.
Table formatting, margin collapsing, floats, transforms, multicolumn,
sticky/fixed positioning, vertical writing, custom/variable fonts, general font
matching, emoji/general script coverage, hyphenation, tailored breaking,
justification, full cross-element shaping, fragment containing blocks and full
painting-context promotion remain deferred. Outline SVG selection/search is
limited. Parser diagnostic gaps remain separately visible. There is no JS,
live DOM/event loop, browser window/chrome, crawler or separate application.

The next concrete Phos milestone is a pinned, licensed CSS interoperability
corpus for the implemented sizing/layout subset, then Grid row-dependent
intrinsic reruns and replaced-element transferred minimum sizes, followed by
fragment containing blocks and positioned-descendant paint promotion without
breaking ancestor clips. This engine foundation is ready for later real UI
integration within the documented subset; broader conformance is not claimed.
