# Pinned Phos rendering fixtures

The eight HTML scenarios (`cards`, `nested`, `mixed`, `text`, `images`,
`malformed`, `clip`, and `start_page`) plus `local-link.html` are checked by
`tests/render_fixtures.rs`. `two-pixels.png` is a self-authored 2×1 RGBA
PNG with one red and one blue pixel. `local.css` tests linked local CSS.

`start_page.html` retains the owner-provided historical canary adapted from
[`research/phos-home/home.html`](https://github.com/saudaljuaid/Scarlite-UI/blob/6773b544e29b56e4bc938d04e84fe8f3e87450bc/research/phos-home/home.html)
in Scarlite-UI at commit `6773b544e29b56e4bc938d04e84fe8f3e87450bc`
(same project owner). Its only historical change is a
responsive `width:100%; max-width:630px` grid in this engine fixture. The UI
repository and its expected `home.svg` were not modified. The upstream UI
repository declares no license at that pin; no upstream open-source license
is inferred. The established owner-provided canary is retained under this
repository's distribution, without copying new UI content. Other historical
rendering fixtures are original project fixtures; no WPT CSS expectations
were copied or edited.

The fixture tests assert exact computed values, box widths and positions,
line fragmentation, image aspect ratios, SVG XML structure, redirect metadata,
template inertness, and repeated bounds. Cards and the start-page canary run
at 320, 640, and 900 CSS pixels.

The text and positioning milestone adds twelve original HTML fixtures:
`unicode`, `bidi`, `inline_geometry`, `relative`, `absolute`, `stacking`,
`position_clip`, `article`, `nav_cards`, `many_spans`, `long_token`, and
`malformed_deep`. These files are authored for Phos and distributed under
the repository's Apache-2.0 license. They contain no copied upstream CSS
expected output, screenshot, or downloaded page content. The two stress
seeds are expanded by the integration tests to exercise thousands of spans
and a long uninterrupted token; the malformed seed also has generated
boundary tests for nesting and repeated invalid declarations.

The complete source inventory is 21 HTML files, one PNG, one CSS file, and
this source note. Visual SVG and PNG review artifacts are produced outside
the repository. The original parser corpora in `tests/upstream` and
`tests/fixtures` remain unchanged.
