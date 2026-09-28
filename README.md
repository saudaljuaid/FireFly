# FireFly

<img src="assets/logo.webp" alt="FireFly logo: a firefly in front of a blue globe" width="320">

**FireFly** is a browser project. **Phos** is its browser engine, written in Rust from the ground up. The project does not embed Chromium, WebKit, or another browser engine.

Phos can now load local HTML or fetch a web page over HTTP or HTTPS, follow redirects, load linked stylesheets, and render the result as SVG. It is still an early engine, not an everyday browser. There is no browser window, JavaScript runtime, image rendering, or full web standards support yet.

## Try it

Install a current Rust toolchain, then run:

```sh
cargo run -- examples/welcome.html --output welcome.svg --width 900
cargo run -- https://example.com/ --output example.svg --width 900
```

Open the SVG output in an image viewer. Page loading, HTML and CSS processing, layout, and painting are implemented in Phos. TLS certificate verification uses Rustls and the Mozilla root set; building a custom cryptography stack is outside the browser engine's scope. The CLI accepts UTF-8 documents up to 16 MiB. The width must be between 1 and 16,384 pixels.

On Windows with the MSVC Rust target, building also requires the Visual Studio C++ Build Tools linker. WSL with a Linux Rust toolchain works as another development environment.

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## How Phos works

```text
file or URL → HTML parser → document tree
                 inline and linked CSS → stylesheet
document tree + stylesheet → computed styles → layout scene → SVG
```

The document arena stores nodes by ID. This keeps relationships explicit and avoids a tree of reference counted pointers. The style pass resolves declarations before layout. Layout produces a scene of rectangles and text, so painting has no need to parse HTML or CSS. Each stage has a separate Rust module and can be replaced or expanded without changing the whole pipeline.

The loader uses HTTP/1.1 with bounded response parsing, timeouts, up to five redirects, chunked decoding, and certificate checked HTTPS. It accepts UTF-8 and uncompressed responses. Linked stylesheets are loaded in document order; resource failures are reported without discarding the page. Cookies, caching, proxies, compressed responses, and CSS `@import` are not implemented.

The renderer supports common elements, text, basic entity decoding, `<style>` elements, inline `style` attributes, tag/class/ID selectors, descendant selectors, simple cascade and inheritance, block flow, text wrapping, colors, fixed pixel sizes, margin, and padding. Unsupported CSS properties and selectors are ignored. SVG text measurement uses an estimate; complex scripts and precise font shaping are not implemented. HTML nesting is limited to 256 elements to bound recursive work.

## Direction

Phos will grow by replacing each provisional piece with standards driven implementations and conformance tests. The next milestones are a spec based HTML tokenizer and tree builder, a fuller CSS cascade and layout model, precise font shaping, image decoding, and an interactive viewport. JavaScript, security boundaries, and browser UI need their own designs and tests.

The goal is an independent, maintainable browser engine. Each milestone should be useful and testable on its own, and documentation should state what works without implying that incomplete features are finished.

## Repository map

| Path | Purpose |
| --- | --- |
| `src/html.rs`, `src/dom.rs` | HTML parsing and document storage |
| `src/url.rs`, `src/network.rs` | URL resolution and HTTP/HTTPS loading |
| `src/css.rs`, `src/style.rs` | CSS parsing and computed styles |
| `src/layout.rs`, `src/paint.rs` | Layout scene and SVG output |
| `src/main.rs` | File and URL rendering command |
| `examples/welcome.html` | Small sample document |
| `assets/logo.webp` | FireFly logo |

Licensed under Apache 2.0. See [LICENSE](LICENSE).
