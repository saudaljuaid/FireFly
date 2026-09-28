# FireFly

<img src="assets/logo.webp" alt="FireFly logo: a firefly in front of a blue globe" width="320">

**FireFly** is a browser project. **Phos** is its browser engine, written in Rust from the ground up. The project does not embed Chromium, WebKit, or another browser engine.

This repository currently contains the **first engine milestone**, not a usable everyday browser. Phos can read a local HTML file, parse a documented subset of HTML and CSS, lay out text and block elements, and render the result as SVG. There is no browser window, network stack, JavaScript runtime, or full web standards support yet.

## Try it

Install a current Rust toolchain, then run:

```sh
cargo run -- examples/welcome.html --output welcome.svg --width 900
```

Open `welcome.svg` in an image viewer. The output comes from Phos's own document, style, layout, and paint code. The CLI accepts local UTF-8 files up to 16 MiB. The width must be between 1 and 16,384 pixels.

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## How Phos works

```text
HTML source → document arena → computed styles → layout scene → SVG
                  ↑                    ↑
             HTML parser          CSS rules and selectors
```

The document arena stores nodes by ID. This keeps relationships explicit and avoids a tree of reference counted pointers. The style pass resolves declarations before layout. Layout produces a scene of rectangles and text, so painting has no need to parse HTML or CSS. Each stage has a separate Rust module and can be replaced or expanded without changing the whole pipeline.

The current implementation supports common elements, text, basic entity decoding, `<style>` elements, inline `style` attributes, tag/class/ID selectors, descendant selectors, simple cascade and inheritance, block flow, text wrapping, colors, fixed pixel sizes, margin, and padding. Unsupported CSS properties and selectors are ignored. SVG text measurement uses an estimate; complex scripts and precise font shaping are not implemented. HTML nesting is limited to 256 elements to bound recursive work.

## Direction

Phos will grow by replacing each provisional piece with standards driven implementations and conformance tests. The next milestones are a spec based HTML tokenizer and tree builder, a fuller CSS cascade and layout model, precise font shaping, image decoding, and an interactive viewport. Networking, JavaScript, security boundaries, and browser UI are later systems with their own designs and tests.

The goal is an independent, maintainable browser engine. Each milestone should be useful and testable on its own, and documentation should state what works without implying that incomplete features are finished.

## Repository map

| Path | Purpose |
| --- | --- |
| `src/html.rs`, `src/dom.rs` | HTML parsing and document storage |
| `src/css.rs`, `src/style.rs` | CSS parsing and computed styles |
| `src/layout.rs`, `src/paint.rs` | Layout scene and SVG output |
| `src/main.rs` | Local rendering command |
| `examples/welcome.html` | Small sample document |
| `assets/logo.webp` | FireFly logo |

Licensed under Apache 2.0. See [LICENSE](LICENSE).
