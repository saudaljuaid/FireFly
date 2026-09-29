# Scarlite

<img src="assets/logo.webp" alt="Scarlite red emblem" width="220">

Scarlite is a browser project built around **Phos**, a Rust HTML and CSS rendering engine. Phos loads local and HTTP(S) pages and renders them to SVG. Scarlite has no browser window yet.

## Try it

```sh
cargo run --locked --bin scarlite -- examples/welcome.html --output welcome.svg
cargo run --locked --bin scarlite -- https://example.com --output example.svg
```

The `librefly` command is also available. Both commands use Phos.

## Features

Phos parses HTML, applies inline and linked CSS, lays out text and boxes, and paints SVG.

## Documentation

See [engine status and test results](docs/ENGINE_STATUS.md) for implementation details and upstream fixture counts. The [upstream fixture notes](tests/upstream/README.md) record their sources and licenses.

## License

Apache 2.0. See [LICENSE](LICENSE).
