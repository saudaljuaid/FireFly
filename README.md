# Scarlite

<img src="assets/logo.webp" alt="Scarlite red emblem" width="320">

**Scarlite** is a browser project. **Phos** is its Rust engine. Phos loads a local HTML file or an HTTP(S) page, builds a document, applies inline and linked CSS, lays out text and boxes, and paints SVG. The library remains named `phos`; both `scarlite` and the existing `librefly` CLI are built. There is no browser window or JavaScript runtime.

## Try it

```sh
cargo run --bin scarlite -- examples/welcome.html --output welcome.svg --width 900
cargo run --bin librefly -- https://example.com --output example.svg --width 900
```

The CLI accepts UTF-8 HTML up to 16 MiB and widths from 1 to 16,384 pixels. HTTPS verifies certificates with Rustls and the Mozilla root set. The loader follows up to five redirects, decodes chunked HTTP/1.1 responses, and loads linked stylesheets in document order. It does not handle compressed responses, cookies, caching, proxies, or CSS `@import`. On Windows, an MSVC build needs the Visual Studio C++ Build Tools linker; a GNU Rust target with MinGW is another option.

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

## HTML parser milestone

The parser follows the [WHATWG HTML Standard's tokenization and tree-construction algorithms](https://html.spec.whatwg.org/multipage/parsing.html). Its default scripting mode is disabled. `parse_with_scripting(input, true)` selects script-enabled parsing rules, such as `noscript` handling, but does not execute scripts.

At the starting `main` commit `924a120`, the existing locked test suite passed. The initial diagnostic inventory on the first 55 WPT files found 1,506 passing and 146 failing document trees; fragment and script-on cases were not yet counted by that pass. The initial 11-file html5lib inventory found 6,905 passing and 63 failing representable tokenizer cases, plus four unrepresentable inputs. The final inventories below include additional upstream files and all fragment and script-on cases, so their totals have a broader scope.

The tokenizer implements the full 2,231-entry named character-reference table, longest-match and attribute-context rules, numeric references and control-code replacement, data/RCDATA/RAWTEXT/PLAINTEXT, script-data escaped and double-escaped states, foreign-content CDATA, comments, DOCTYPE identifiers and quirks recovery, attributes, end-tag matching, and EOF recovery. It normalizes CR and CRLF to LF. The current tokenizer recognizes processing instructions; `Tokenizer::new_legacy_html5lib` exposes the older bogus-comment behavior needed to compare the historical html5lib tokenizer corpus without changing its expected tokens.

Tree construction implements the `initial`, `before html`, `before head`, `in head`, `in head noscript`, `after head`, `in body`, `text`, `in table`, `in table text`, `in caption`, `in column group`, `in table body`, `in row`, `in cell`, `in template`, `after body`, `after after body`, `in frameset`, `after frameset`, and `after after frameset` insertion modes. It handles implied elements and end tags, table foster parenting, nested tables, select/table transitions, the form-element pointer, frameset replacement, head noscript, active-formatting reconstruction and the bounded adoption-agency algorithm, template mode stacks, and EOF in these modes. SVG and MathML nodes retain their namespaces; foreign-content breakout, integration points, name and attribute adjustments, and CDATA dispatch follow the corresponding rules. Template contents live under a distinct inert arena node; static `selectedcontent` contents are synchronized from the selected option.

`phos::html::parse(input)` returns a complete `Document`. `phos::html::parse_fragment(input, &context)` returns a `Document` whose root children are the fragment nodes; the context element itself is omitted. The context is a `phos::dom::Element` with a namespace, tag, and attributes. Fragment parsing initializes the tokenizer and insertion mode for contexts including `table`, `select`, `textarea`, `script`, `svg`, `math`, and `template`; a `form` context also initializes the form pointer. `parse_fragment_with_scripting` accepts a parser scripting flag. The APIs preserve the 16 MiB input limit, 256-open-element limit, and 32-step bound on reprocessing one token. Repeated malformed-input and boundary tests check those limits and arena consistency.

The arena stores parent/child links, namespaces, attributes, comments, doctypes, processing instructions, text, and template content. After parsing it retains only reachable nodes, so reparenting and frameset replacement leave no detached internal nodes. Every upstream exact-tree test also checks ownership, unique attributes, parent links, cycles, and template-content placement. Inline and linked stylesheets, local and HTTPS rendering, and visible SVG text were verified through the existing library and CLI paths.

### Upstream tokenizer results

The 13 original `.test` files with `tests` arrays from [html5lib-tests](https://github.com/html5lib/html5lib-tests/tree/224991ec10db04f056a89eed8b0bd8695fd2950e/tokenizer) are checked across every declared initial state. “Current” uses Phos's default tokenizer; “legacy” uses its explicit html5lib compatibility mode. The current mode has 11 failures, all historical `<?` bogus-comment expectations superseded by current processing-instruction behavior. Four inputs contain unpaired UTF-16 surrogates and cannot be expressed through Phos's UTF-8 `&str` API. These are counted as unrepresentable in both modes. Token sequences are compared exactly; parse-error lists are not compared.

| Fixture | Current pass | Current fail | Legacy pass | Legacy fail | Unrepresentable |
| --- | ---: | ---: | ---: | ---: | ---: |
| `contentModelFlags.test` | 24 | 0 | 24 | 0 | 0 |
| `domjs.test` | 59 | 0 | 59 | 0 | 0 |
| `entities.test` | 80 | 0 | 80 | 0 | 0 |
| `escapeFlag.test` | 9 | 0 | 9 | 0 | 0 |
| `namedEntities.test` | 4210 | 0 | 4210 | 0 | 0 |
| `numericEntities.test` | 336 | 0 | 336 | 0 | 0 |
| `pendingSpecChanges.test` | 1 | 0 | 1 | 0 | 0 |
| `test1.test` | 69 | 0 | 69 | 0 | 0 |
| `test2.test` | 43 | 2 | 45 | 0 | 0 |
| `test3.test` | 1777 | 9 | 1786 | 0 | 0 |
| `test4.test` | 85 | 0 | 85 | 0 | 0 |
| `unicodeChars.test` | 323 | 0 | 323 | 0 | 0 |
| `unicodeCharsProblematic.test` | 1 | 0 | 1 | 0 | 4 |
| **Total** | **7017** | **11** | **7028** | **0** | **4** |

### Upstream tree-construction results

All 58 non-script-executing `.dat` files from the [WPT parsing resources](https://github.com/web-platform-tests/wpt/tree/7a8d143bce12c1142109d27a8de4b0b26f9f5a47/html/syntax/parsing/resources) are copied intact, including three files marked “unsafe” for the browser test harness. Complete expected trees are compared exactly. “Script on” selects parsing rules only; the four upstream `scripted_*.dat` files require JavaScript execution and are outside this milestone. `0/0` means a fixture has no case of that kind. All 1,953 included cases pass, with zero failing tree comparisons.

| Fixture | Document pass/fail | Fragment pass/fail | Script on pass/fail |
| --- | ---: | ---: | ---: |
| `adoption01.dat` | 17/0 | 1/0 | 0/0 |
| `adoption02.dat` | 4/0 | 0/0 | 0/0 |
| `blocks.dat` | 48/0 | 0/0 | 0/0 |
| `comments01.dat` | 16/0 | 0/0 | 0/0 |
| `doctype01.dat` | 37/0 | 0/0 | 0/0 |
| `domjs-unsafe.dat` | 49/0 | 0/0 | 0/0 |
| `entities01.dat` | 75/0 | 0/0 | 0/0 |
| `entities02.dat` | 26/0 | 0/0 | 0/0 |
| `foreign-fragment.dat` | 0/0 | 66/0 | 0/0 |
| `html5test-com.dat` | 30/0 | 0/0 | 0/0 |
| `inbody01.dat` | 4/0 | 0/0 | 0/0 |
| `isindex.dat` | 4/0 | 0/0 | 0/0 |
| `main-element.dat` | 3/0 | 0/0 | 0/0 |
| `math.dat` | 0/0 | 8/0 | 0/0 |
| `menuitem-element.dat` | 20/0 | 0/0 | 0/0 |
| `namespace-sensitivity.dat` | 1/0 | 0/0 | 0/0 |
| `noscript01.dat` | 18/0 | 0/0 | 0/0 |
| `pending-spec-changes-plain-text-unsafe.dat` | 1/0 | 0/0 | 0/0 |
| `pending-spec-changes.dat` | 3/0 | 0/0 | 0/0 |
| `plain-text-unsafe.dat` | 37/0 | 11/0 | 0/0 |
| `processing-instructions.dat` | 124/0 | 0/0 | 0/0 |
| `quirks01.dat` | 4/0 | 0/0 | 0/0 |
| `ruby.dat` | 21/0 | 0/0 | 0/0 |
| `scriptdata01.dat` | 26/0 | 0/0 | 0/0 |
| `search-element.dat` | 3/0 | 0/0 | 0/0 |
| `svg.dat` | 0/0 | 8/0 | 0/0 |
| `tables01.dat` | 19/0 | 0/0 | 0/0 |
| `template.dat` | 123/0 | 4/0 | 0/0 |
| `tests1.dat` | 112/0 | 0/0 | 0/0 |
| `tests10.dat` | 54/0 | 0/0 | 0/0 |
| `tests11.dat` | 13/0 | 0/0 | 0/0 |
| `tests12.dat` | 2/0 | 0/0 | 0/0 |
| `tests14.dat` | 7/0 | 0/0 | 0/0 |
| `tests15.dat` | 14/0 | 0/0 | 0/0 |
| `tests16.dat` | 191/0 | 0/0 | 6/0 |
| `tests17.dat` | 13/0 | 0/0 | 0/0 |
| `tests18.dat` | 36/0 | 0/0 | 0/0 |
| `tests19.dat` | 103/0 | 0/0 | 0/0 |
| `tests2.dat` | 63/0 | 0/0 | 0/0 |
| `tests20.dat` | 64/0 | 0/0 | 0/0 |
| `tests21.dat` | 23/0 | 0/0 | 0/0 |
| `tests22.dat` | 5/0 | 0/0 | 0/0 |
| `tests23.dat` | 5/0 | 0/0 | 0/0 |
| `tests24.dat` | 8/0 | 0/0 | 0/0 |
| `tests25.dat` | 26/0 | 0/0 | 0/0 |
| `tests26.dat` | 20/0 | 0/0 | 0/0 |
| `tests3.dat` | 24/0 | 0/0 | 0/0 |
| `tests4.dat` | 0/0 | 9/0 | 0/0 |
| `tests5.dat` | 16/0 | 0/0 | 1/0 |
| `tests6.dat` | 39/0 | 13/0 | 0/0 |
| `tests7.dat` | 33/0 | 1/0 | 0/0 |
| `tests8.dat` | 10/0 | 0/0 | 0/0 |
| `tests9.dat` | 27/0 | 0/0 | 0/0 |
| `tests_innerHTML_1.dat` | 0/0 | 81/0 | 0/0 |
| `tricky01.dat` | 9/0 | 0/0 | 0/0 |
| `void-in-phrasing.dat` | 13/0 | 0/0 | 0/0 |
| `webkit01.dat` | 52/0 | 0/0 | 0/0 |
| `webkit02.dat` | 44/0 | 4/0 | 1/0 |
| **Total** | **1739/0** | **206/0** | **8/0** |

The original fixtures and their [source and license details](tests/upstream/README.md) are preserved. The older focused fixtures in `tests/fixtures` also remain unchanged. These results cover the listed upstream cases, not every possible HTML input or every browser behavior.

## Current limits and direction

Phos does not yet report parse errors, sniff encodings or accept a byte-stream input, execute scripts or perform script-driven parser mutations, or provide live DOM behavior such as form association and dynamic `selectedcontent` updates. The fragment API receives one context element, so it cannot infer a `form` ancestor of that element. The tokenizer's UTF-8 API cannot represent unpaired UTF-16 surrogates. Full HTML conformance has not been established. The next parser step is to compare and expose parse errors and to add a byte-stream frontend with encoding preprocessing; script-driven tree construction needs a JavaScript integration design before the four scripted WPT files can be exercised.

Rendering supports common elements, CSS declarations and selectors, block flow, text wrapping, and SVG output. Unsupported CSS rules and properties are ignored. SVG text width is estimated; precise font shaping, image rendering, and an interactive viewport remain future work.

## Repository map

| Path | Purpose |
| --- | --- |
| `src/html/tokenizer.rs`, `src/html/named_references.rs`, `src/html.rs` | HTML tokenization and tree construction |
| `src/dom.rs` | Document arena and relationships |
| `src/network.rs`, `src/url.rs` | HTTP(S) loading and URL resolution |
| `src/css.rs`, `src/style.rs` | CSS parsing and computed styles |
| `src/layout.rs`, `src/paint.rs` | Layout scene and SVG output |
| `src/main.rs` | `scarlite` and `librefly` CLI |
| `examples/welcome.html` | Small sample document |
| `assets/logo.webp` | Scarlite logo |
| `tests/upstream` | Pinned upstream parser corpora |
| `tools/generate_named_references.py` | Regenerate the WHATWG named-reference table |

Licensed under Apache 2.0. See [LICENSE](LICENSE).
