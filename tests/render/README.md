# Pinned Phos rendering fixtures

The eight HTML scenarios (`cards`, `nested`, `mixed`, `text`, `images`,
`malformed`, `clip`, and `start_page`) plus `local-link.html` are checked by
`tests/render_fixtures.rs`. `two-pixels.png` is a self-authored 2×1 RGBA
PNG with one red and one blue pixel. `local.css` tests linked local CSS.

`start_page.html` adapts `research/phos-home/home.html` from the Scarlite-UI
repository at commit `6773b54` (same project owner). Its only change is a
responsive `width:100%; max-width:630px` grid in this engine fixture. The UI
repository and its expected `home.svg` were not modified. The remaining
rendering fixtures are original to this milestone; no WPT CSS expectations
were copied or edited.

The fixture tests assert exact computed values, box widths and positions,
line fragmentation, image aspect ratios, SVG XML structure, redirect metadata,
template inertness, and repeated bounds. Cards and the start-page canary run
at 320, 640, and 900 CSS pixels.
