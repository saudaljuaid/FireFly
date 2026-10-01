# Cached documents for native embedders

Phos remains a headless engine. `LoadedDocument` separates resource I/O and HTML
parsing from viewport-dependent CSS, layout and SVG generation. The original
`render`, `render_bytes`, `render_file` and `render_url` APIs retain their behavior.

```rust
let page = phos::LoadedDocument::from_file(std::path::Path::new("examples/welcome.html"))?;
let svg = page.render(phos::Viewport { width: 900.0, height: Some(600.0) })?;
let narrow = page.render(phos::Viewport { width: 320.0, height: Some(600.0) })?;
# Ok::<(), phos::Error>(())
```

`from_url` accepts HTTP(S); `from_file` resolves local resources against the
document directory. `from_html` and `from_bytes` match the in-memory APIs and
do not load external resources. Public metadata includes `final_url` (after
redirects, absent for local input), a bounded decoded HTML title, resource
warnings, and the number of retained linked stylesheets. `render` performs no
network or filesystem I/O. A reload requires loading a new document; an embedder
owns navigation history and scrolling independently of this cache.

The viewport uses CSS pixels. Its width must be the available document width,
after reserving chrome and scrollbar space. Its optional height describes the
actual available screen environment for height units; it never describes the
final document extent. Both dimensions use the existing 1–16,384 limit. SVG
continues to cover the complete bounded document; an embedder must clip and
rasterize only a bounded visible region rather than allocate its full height.

Inline and linked stylesheet media attributes are evaluated on each render.
Linked sheets are loaded once even when their media condition is initially
inactive, preserving their source order for later breakpoint changes. All
retained sources, including inactive media and separators, share the existing
4 MiB combined cap; a rejected oversized source does not prevent a later small
inline source from being retained. Loading is capped at 16 stylesheet requests,
2 MiB per linked sheet and 16 image requests. HTTP, redirect, TLS, MIME, encoding,
HTML (16 MiB), image (4 MiB compressed, 16 million decoded pixels, 64 MiB decoder
allocation), CSS parsing, cascade, layout and SVG limits remain in force.
Retained images are the original bounded data URLs, not decoded full-page pixel
buffers. This is a per-document resource cache, not an HTTP or persistent cache.

`render_with_timings` returns the same SVG and `RenderTimings` durations for
active stylesheet selection/parsing, computed styles plus layout, and SVG
serialization. It excludes loading, SVG parsing, rasterization and presentation.

## Release measurements

Measured with Linux Rust 1.98.1 under WSL Ubuntu 24.04 on the development host,
`--release`, at an explicit screen height of 600 CSS pixels. Each width receives
one warmup and ten measured cached renders. These are engineering observations,
not portable speed guarantees; host scheduling affects the maxima. Original
fixtures and `tools/measure_layout.py` sources were preserved unchanged.

| Original source | Width | Layout median ms | SVG median ms | Total median ms |
| --- | ---: | ---: | ---: | ---: |
| Article | 320 / 900 | 1.02 / 1.17 | 2.83 / 3.08 | 3.84 / 4.22 |
| Flexbox landing | 320 / 900 | 2.56 / 2.28 | 2.60 / 2.32 | 5.46 / 5.03 |
| Grid dashboard | 320 / 900 | 1.77 / 1.63 | 1.72 / 1.64 | 3.54 / 3.44 |
| Nested boxes | 320 / 900 | 0.03 / 0.04 | 0.08 / 0.11 | 0.11 / 0.16 |
| Start-page canary | 320 / 900 | 0.10 / 0.13 | 0.48 / 0.44 | 0.61 / 0.57 |
| 150 Unicode paragraphs | 320 / 900 | 10.00 / 10.04 | 12.35 / 12.15 | 22.10 / 22.18 |
| 1,200 flexible items | 320 / 900 | 42.26 / 30.72 | 41.00 / 30.58 | 85.22 / 62.80 |
| 1,200 grid items | 320 / 900 | 57.13 / 33.36 | 27.33 / 23.40 | 84.74 / 56.86 |
| 64 alternating nested containers | 320 / 900 | 0.65 / 1.01 | 0.21 / 0.22 | 0.87 / 1.28 |

Total includes the separate stylesheet stage; stage medians need not sum to the
median total. The stress pages exceed a 16 ms input frame before rasterization,
supporting a background resize worker in a native host. The host should coalesce
requests and reject stale results while retaining window operations on its event
thread. This API does not own that worker or any windowing dependencies.

```sh
cargo run --locked --release --example measure_cached
cargo build --locked --release --bin scarlite
python3 tools/measure_layout.py --binary target/release/scarlite --output-dir /outside/repository/stress --samples 1 --width 640
cargo run --locked --release --example measure_cached -- /outside/repository/stress/sources/text-heavy.html /outside/repository/stress/sources/many-flex.html /outside/repository/stress/sources/many-grid.html /outside/repository/stress/sources/deep-nested.html
```

The example emits every sample as CSV and writes no artifacts in the checkout.
Regression tests exercise cached resize after source deletion or server closure,
media changes, redirect/encoding metadata, resource limits, viewport validation,
explicit height semantics and exact output equality with the original APIs on
the five original representative fixtures and start-page canary. Baseline was
367 passed, zero failed and three intentionally ignored; this addition brings
the locked suite to 373 passed with the same ignored parser diagnostics.
