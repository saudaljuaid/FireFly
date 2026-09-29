# LibreFly

<img src="assets/logo-readme.jpeg" alt="LibreFly logo: an orange winged insect on a blue globe" width="320">

**LibreFly** is a browser project. **Phos** is its browser engine, written in Rust from the ground up. The project does not embed Chromium, WebKit, or another browser engine.

Phos can load local HTML or fetch a web page over HTTP or HTTPS, follow redirects, load linked stylesheets, and render the result as SVG. It is still an early engine, not an everyday browser. There is no browser window, JavaScript runtime, image rendering, or full web standards support yet.

## Try it

Install a current Rust toolchain, then run:

```sh
cargo run -- examples/welcome.html --output welcome.svg --width 900
cargo run -- https://httpbin.org/html --output page.svg --width 900
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
file or URL → HTML tokenizer → tree builder → document tree
                 inline and linked CSS → stylesheet
document tree + stylesheet → computed styles → layout scene → SVG
```

The document arena stores nodes by ID. This keeps relationships explicit and avoids a tree of reference counted pointers. The style pass resolves declarations before layout. Layout produces a scene of rectangles and text, so painting has no need to parse HTML or CSS. Each stage has a separate Rust module and can be replaced or expanded without changing the whole pipeline.

The loader uses HTTP/1.1 with bounded response parsing, timeouts, up to five redirects, chunked decoding, and certificate checked HTTPS. It accepts UTF-8 and uncompressed responses. Linked stylesheets are loaded in document order; resource failures are reported without discarding the page. Cookies, caching, proxies, compressed responses, and CSS `@import` are not implemented.

The tokenizer in `src/html/tokenizer.rs` emits DOCTYPE, start tag, end tag, comment, character, and EOF tokens. Tags carry attributes and a self-closing flag. The tree builder in `src/html.rs` consumes one token at a time and owns insertion-mode state. When it inserts a text element, it switches the tokenizer to RCDATA (`title`, `textarea`), RAWTEXT (`style`, `xmp`, `iframe`, `noembed`, `noframes`), or script data, then restores its previous insertion mode at the closing tag or EOF. A `table` start tag changes the tree builder from `in body` to `in table`. Captions switch to `in caption`; explicit `colgroup` and implied `colgroup` for a bare `col` switch to `in column group`. Sections, rows, and cells use `in table body`, `in row`, and `in cell`. Characters directly under a table, section, or row enter `in table text` until the next non-character token. The pending text is then inserted or foster parented as a group, and that token is reprocessed in the previous mode. HTML input remains limited to 16 MiB and nesting to 256 open elements; token reprocessing has a 32-step bound.

Tree construction implements the [HTML Living Standard](https://html.spec.whatwg.org/multipage/parsing.html#tree-construction) `initial`, `before html`, `before head`, `in head`, `after head`, `in body`, `text`, and `in template` insertion modes, plus the `after body` and `after after body` epilogue modes. It creates implied `html`, `head`, and `body` elements, preserves explicit ones, and stores comments and doctypes without rendering them. Head handling covers `base`, `link`, `meta`, `title`, `style`, and `script`, including head content just after `</head>`. In the body, a new paragraph or relevant block closes an open `p`; a new `li` closes the previous list item; unmatched end tags are ignored where the in-body rules require it. A stray `</p>` creates and closes an empty paragraph. Self-closing flags on non-void HTML elements are ignored. Parse errors are recovered where implemented but are not reported.

The table pass implements the core `in table`, `in table text`, `in caption`, `in column group`, `in table body`, `in row`, and `in cell` modes. It inserts explicit sections, rows, cells, captions, column groups, and columns; it implies `tbody` and `tr` when needed and implies `colgroup` for a bare `col`. Column attributes stay on `col`, and whitespace and comments within a column group stay there. Other content closes the column group and is reprocessed in `in table`. Caption content uses body rules; a following caption, column, section, row, cell, or `</table>` closes the caption and reprocesses that token. Table, section, row, cell, caption, and column-group end tags close the appropriate open elements; EOF leaves a partial table in the document. Whitespace-only table text stays in the table; a run containing other characters is foster parented before the nearest table. Unexpected elements directly in a table use the same foster-parenting path. Existing local and loaded-page rendering and stylesheet processing use the resulting document tree.

The current standard handles `select`, `option`, and `optgroup` in the body rules, including implied option endings, an incoming `select` closing an open select, and `input` ending a select. Table tokens around a select follow table insertion and foster-parenting rules; this covers the selected legacy select-in-table cases without a separate select insertion mode. `hr` uses the select-specific implied-end-tag step. Scope checks keep select handling from crossing a template boundary.

SVG and MathML elements carry an explicit namespace in the arena. Foreign-content dispatch handles text, comments, self-closing tags, matching end tags, and HTML breakout tags. It adjusts the specified SVG element and attribute names, MathML `definitionURL`, and recognized XLink, XML, and XMLNS attributes. MathML text integration points, `annotation-xml` with HTML encoding, and SVG `foreignObject`, `desc`, and `title` return eligible tokens to HTML insertion rules. Scope, special-element, and foster-parenting checks account for namespaces.

HTML templates have a stack of template insertion modes, formatting markers, and a content fragment in the arena. Template table tokens select the appropriate table mode; closing tags and EOF unwind nested templates and reset the insertion mode. Content under the fragment is inert for rendering and inline stylesheet collection. The arena's fragment node models the template's content relationship for tree comparisons and parent-link checks.

The tree builder now keeps the [list of active formatting elements](https://html.spec.whatwg.org/multipage/parsing.html#the-list-of-active-formatting-elements) with markers for captions, cells, applets, marquees, and objects. It reconstructs formatting for body text and phrasing content, clears entries at marker boundaries, and keeps at most three identical entries after the last marker. Formatting end tags use the [adoption agency algorithm](https://html.spec.whatwg.org/multipage/parsing.html#adoption-agency-algorithm): it delegates when no active entry exists, reparents nodes through the document arena and foster-parenting insertion location, and bounds its outer loop to eight iterations. The inner loop walks the bounded open-element stack and applies the standard's three-entry rule.

This remains a partial HTML parser. Head `noscript`, framesets, form-element pointer behavior, fragment parsing, and many unselected tree-construction cases remain unsupported or unverified. Table handling does not imply full table conformance. The tokenizer still lacks the full named character reference table, script escaped and double-escaped states, CDATA, and some comment and DOCTYPE recovery states. The 16 MiB input, 256-open-element, and 32-step token-reprocessing bounds remain in place.

Focused token tests compare selected cases from the MIT-licensed [html5lib-tests tokenizer corpus](https://github.com/html5lib/html5lib-tests/tree/master/tokenizer); its notice is in `tests/html5lib-LICENSE`. Selected `test1.test` basic tag, attribute, comment, DOCTYPE, and EOF cases pass. Selected `numericEntities.test` overflow, null, and Windows-1252 cases and `entities.test` unknown-name and attribute cases pass. A RAWTEXT end-tag case from `contentModelFlags.test` also passes. The remaining cases in those files and the other tokenizer fixture groups have not been established as passing; in particular, the full named-entity and script-escape groups remain unsupported.

Tree tests compare document shape against **278 exact cases** from the html5lib tree-construction corpus, [now maintained in web-platform-tests](https://github.com/web-platform-tests/wpt/tree/master/html/syntax/parsing/resources). The selected case numbers are:

| Fixture | Passing case numbers |
| --- | --- |
| `tests1.dat` | #1–10, #34, #50, #55, #84–86, #88 |
| `tests1-select.dat` (from `tests1.dat`) | #30, #35, #100 |
| `tests2.dat` | #37–39, #49 |
| `blocks.dat`, `inbody01.dat`, `scriptdata01.dat`, `doctype01.dat`, `comments01.dat` | #1–4; #1–2; #1–2; #1–2; #1 respectively |
| `tables01.dat` | #1–19, including the formerly omitted #7–10 and #17–18 |
| `adoption01.dat`, `adoption02.dat` | #1–16, including the formerly omitted #13; #1–4 respectively |
| `tests17.dat` | #1–12 |
| `tests9.dat` | #1–21, #24–26 |
| `tests10.dat` | #1, #3–20, #23–25 |
| `tests11.dat` | #1, #3, #5–7, #9–10, #13 |
| `tests20.dat` | #53–64 |
| `webkit02.dat` | #23–44 |
| `template.dat` | #1–39, #50–73, #75–99, #101–113, #125–127 |

All selected cases pass as exact tree comparisons. Source input and expected trees are in `tests/fixtures`, with the original MIT notice in `tests/html5lib-LICENSE` and the [web-platform-tests BSD notice](https://github.com/web-platform-tests/wpt/blob/master/LICENSE.md) in `tests/wpt-LICENSE`. Assertions compare trees, not parse-error counts, and make no claim about conformance beyond the listed cases. Parent/child consistency is checked for every case, including detached nodes after reparenting. Additional focused tests cover select/table and foreign recovery, template EOF and inertness, formatting reconstruction, nested tables, malformed endings, repeated adversarial input, and the input and nesting limits.

The renderer supports common elements, text, `<style>` elements, inline `style` attributes, tag/class/ID selectors, descendant selectors, simple cascade and inheritance, block flow, text wrapping, colors, fixed pixel sizes, margin, and padding. Unsupported CSS properties and selectors are ignored. SVG text measurement uses an estimate; complex scripts and precise font shaping are not implemented.

## Direction

Phos will grow by replacing each provisional piece with standards driven implementations and conformance tests. Remaining parser work includes head `noscript`, framesets, form handling, fragment parsing, more table and adoption cases, and broader tokenizer coverage. A fuller CSS cascade and layout model, precise font shaping, image decoding, and an interactive viewport remain future work. JavaScript, security boundaries, and browser UI need their own designs and tests.

The goal is an independent, maintainable browser engine. Each milestone should be useful and testable on its own, and documentation should state what works without implying that incomplete features are finished.

## Repository map

| Path | Purpose |
| --- | --- |
| `src/html/tokenizer.rs`, `src/html.rs`, `src/dom.rs` | HTML tokenization, tree building, and document storage |
| `src/url.rs`, `src/network.rs` | URL resolution and HTTP/HTTPS loading |
| `src/css.rs`, `src/style.rs` | CSS parsing and computed styles |
| `src/layout.rs`, `src/paint.rs` | Layout scene and SVG output |
| `src/main.rs` | File and URL rendering command |
| `examples/welcome.html` | Small sample document |
| `assets/logo.jpeg`, `assets/logo-readme.jpeg` | Original logo and cropped README image |

Licensed under Apache 2.0. See [LICENSE](LICENSE).
