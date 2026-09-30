# Engine status

## Input and loading

The CLI accepts HTML byte streams up to 16 MiB and widths from 1 to 16,384 pixels. HTTPS verifies certificates with Rustls and the Mozilla root set. The loader follows up to five redirects, decodes chunked HTTP/1.1 responses, uses the **final** response's Content-Type for HTML encoding, and loads linked stylesheets in document order. Local files are read as bytes. It does not handle compressed responses, cookies, caching, proxies, or CSS `@import`. On Windows, an MSVC build needs the Visual Studio C++ Build Tools linker; a GNU Rust target with MinGW is another option.

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

## HTML parser milestone

The parser follows the [WHATWG HTML Standard's tokenization and tree-construction algorithms](https://html.spec.whatwg.org/multipage/parsing.html). Its default scripting mode is disabled. `parse_with_scripting(input, true)` selects script-enabled parsing rules, such as `noscript` handling, but does not execute scripts.

This milestone starts from `main` commit `ca6b486`. Its full locked suite passed before these changes: 7,028 exact tokenizer sequences, 1,739 exact document trees, 206 exact fragment trees, eight exact script-on trees, and 7,028 exact tokenizer error lists. The initial historical WPT error inventory was 518/263/958 for documents, 38/14/154 for fragments, and 8/0/0 for script-on cases (pass/fail/unrepresentable).

The tokenizer implements the full 2,231-entry named character-reference table, longest-match and attribute-context rules, numeric references and control-code replacement, data/RCDATA/RAWTEXT/PLAINTEXT, script-data escaped and double-escaped states, foreign-content CDATA, comments, DOCTYPE identifiers and quirks recovery, attributes, end-tag matching, and EOF recovery. It normalizes CR and CRLF to LF. The current tokenizer recognizes processing instructions; `Tokenizer::new_legacy_html5lib` exposes the older bogus-comment behavior needed to compare the historical html5lib tokenizer corpus without changing its expected tokens.

Tree construction implements the `initial`, `before html`, `before head`, `in head`, `in head noscript`, `after head`, `in body`, `text`, `in table`, `in table text`, `in caption`, `in column group`, `in table body`, `in row`, `in cell`, `in template`, `after body`, `after after body`, `in frameset`, `after frameset`, and `after after frameset` insertion modes. It handles implied elements and end tags, table foster parenting, nested tables, select/table transitions, the form-element pointer, frameset replacement, head noscript, active-formatting reconstruction and the bounded adoption-agency algorithm, template mode stacks, and EOF in these modes. SVG and MathML nodes retain their namespaces; foreign-content breakout, integration points, name and attribute adjustments, and CDATA dispatch follow the corresponding rules. Template contents live under a distinct inert arena node; static `selectedcontent` contents are synchronized from the selected option.

`phos::html::parse(input)` and `parse_fragment(input, &context)` retain their UTF-8 `&str` APIs. `parse_bytes(bytes, transport_content_type)` and `parse_fragment_bytes(bytes, &context, transport_content_type)` accept byte streams; scripting and diagnostic variants are also public. Pass the **final** HTTP response's complete Content-Type value for network input, and `None` for a local file. `decode_html_bytes` exposes the chosen encoding, its source, and whether replacement occurred. The returned fragment document contains the fragment nodes as root children and omits the context element. Fragment parsing initializes tokenizer and insertion modes for table, select, text, foreign, and template contexts; a form context initializes the form pointer.

Byte decoding follows [HTML encoding sniffing and encoding changes](https://html.spec.whatwg.org/multipage/parsing.html#determining-the-character-encoding) and the [Encoding Standard](https://encoding.spec.whatwg.org/): BOM first with certain confidence, supported charset from the **final** transport response next with certain confidence, then a 1,024-byte meta or XML-declaration prescan with tentative confidence, then an explicit tentative UTF-8 default. `encoding_rs` supplies WHATWG aliases and decoder tables; malformed or truncated sequences become U+FFFD. A valid in-tree `meta charset` or `meta http-equiv="Content-Type"` declaration can change a tentative encoding even when it appears beyond the prescan boundary. The document byte API retains the original bytes, discards the first tree and diagnostics, and decodes and constructs the tree once more with certain confidence. A matching declaration makes confidence certain without restarting; a conflicting later declaration cannot cause a loop. UTF-16BE/LE already in use ignores a conflicting declaration; a declared UTF-16 label maps to UTF-8 and `x-user-defined` maps to windows-1252. An invalid `charset` attribute can fall back to a valid `http-equiv` declaration on the same element. BOM and transport charset cannot be overridden by meta. `decode_html_bytes` reports the initial sniffed choice; the document parse functions apply any restart. Byte-fragment parsing uses the initial sniffed decode without an in-tree restart, and callers passing decoded `&str` have no byte stream to restart. CR and CRLF become LF before tokenization. The byte and decoded UTF-8 inputs each obey the 16 MiB limit; the parser retains its 256-open-element and 32-step token-reprocessing bounds.

`parse_with_errors` and its byte and fragment variants return a `ParseReport { document, errors }`. Each diagnostic has a stable kebab-case code, an input/tokenizer/tree phase, and a position after newline normalization and BOM removal. Offset counts Unicode scalar values from zero; line and UTF-16-code-unit column count from one. Token positions refer to the tokenizer's position at token completion; table-text recovery uses the buffered character token's position. Reprocessing keeps that position and suppresses duplicate reports for one token and code, while nested templates can each report their own EOF recovery error. Diagnostics never change the recovered tree. WPT comparison uses its one-based `(line,column)` coordinates after CRLF normalization; prose locations and historical names without an equivalent code are unrepresentable, while representable list and position differences are failures.

The arena stores parent/child links, namespaces, attributes, comments, doctypes, processing instructions, text, and template content. After parsing it retains only reachable nodes, so reparenting and frameset replacement leave no detached internal nodes. Every upstream exact-tree test also checks ownership, unique attributes, parent links, cycles, and template-content placement. Inline and linked stylesheets, local and HTTPS rendering, and visible SVG text were verified through the library and both CLI names. HTTP stylesheets and base URLs inside template content are ignored.

### Upstream tokenizer results

The 13 original `.test` files with `tests` arrays from [html5lib-tests](https://github.com/html5lib/html5lib-tests/tree/224991ec10db04f056a89eed8b0bd8695fd2950e/tokenizer) are checked across every declared initial state. “Current” uses Phos's default tokenizer with 11 explicitly identified current-standard processing-instruction expectations; a raw comparison against the untouched historical output is 7,017 pass and 11 fail. “Legacy” uses explicit html5lib compatibility behavior and matches all representable original outputs. Four inputs contain unpaired UTF-16 surrogates and cannot be expressed through Phos's UTF-8 `&str` API.

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
| `test2.test` | 45 | 0 | 45 | 0 | 0 |
| `test3.test` | 1786 | 0 | 1786 | 0 | 0 |
| `test4.test` | 85 | 0 | 85 | 0 | 0 |
| `unicodeChars.test` | 323 | 0 | 323 | 0 | 0 |
| `unicodeCharsProblematic.test` | 1 | 0 | 1 | 0 | 4 |
| **Total** | **7028** | **0** | **7028** | **0** | **4** |

### Upstream tree-construction results

All 58 non-script-executing `.dat` files from the [WPT parsing resources](https://github.com/web-platform-tests/wpt/tree/7a8d143bce12c1142109d27a8de4b0b26f9f5a47/html/syntax/parsing/resources) are copied intact, including three files marked “unsafe” for the browser test harness. Complete expected trees are compared exactly. “Script on” selects parsing rules only; the four upstream `scripted_*.dat` files require JavaScript execution and are outside this milestone. `0/0/0` means a fixture has no case of that kind. All 1,953 included cases pass, with zero failing or unrepresentable tree comparisons.

| Fixture | Document P/F/U | Fragment P/F/U | Script on P/F/U |
| --- | ---: | ---: | ---: |
| `adoption01.dat` | 17/0/0 | 1/0/0 | 0/0/0 |
| `adoption02.dat` | 4/0/0 | 0/0/0 | 0/0/0 |
| `blocks.dat` | 48/0/0 | 0/0/0 | 0/0/0 |
| `comments01.dat` | 16/0/0 | 0/0/0 | 0/0/0 |
| `doctype01.dat` | 37/0/0 | 0/0/0 | 0/0/0 |
| `domjs-unsafe.dat` | 49/0/0 | 0/0/0 | 0/0/0 |
| `entities01.dat` | 75/0/0 | 0/0/0 | 0/0/0 |
| `entities02.dat` | 26/0/0 | 0/0/0 | 0/0/0 |
| `foreign-fragment.dat` | 0/0/0 | 66/0/0 | 0/0/0 |
| `html5test-com.dat` | 30/0/0 | 0/0/0 | 0/0/0 |
| `inbody01.dat` | 4/0/0 | 0/0/0 | 0/0/0 |
| `isindex.dat` | 4/0/0 | 0/0/0 | 0/0/0 |
| `main-element.dat` | 3/0/0 | 0/0/0 | 0/0/0 |
| `math.dat` | 0/0/0 | 8/0/0 | 0/0/0 |
| `menuitem-element.dat` | 20/0/0 | 0/0/0 | 0/0/0 |
| `namespace-sensitivity.dat` | 1/0/0 | 0/0/0 | 0/0/0 |
| `noscript01.dat` | 18/0/0 | 0/0/0 | 0/0/0 |
| `pending-spec-changes-plain-text-unsafe.dat` | 1/0/0 | 0/0/0 | 0/0/0 |
| `pending-spec-changes.dat` | 3/0/0 | 0/0/0 | 0/0/0 |
| `plain-text-unsafe.dat` | 37/0/0 | 11/0/0 | 0/0/0 |
| `processing-instructions.dat` | 124/0/0 | 0/0/0 | 0/0/0 |
| `quirks01.dat` | 4/0/0 | 0/0/0 | 0/0/0 |
| `ruby.dat` | 21/0/0 | 0/0/0 | 0/0/0 |
| `scriptdata01.dat` | 26/0/0 | 0/0/0 | 0/0/0 |
| `search-element.dat` | 3/0/0 | 0/0/0 | 0/0/0 |
| `svg.dat` | 0/0/0 | 8/0/0 | 0/0/0 |
| `tables01.dat` | 19/0/0 | 0/0/0 | 0/0/0 |
| `template.dat` | 123/0/0 | 4/0/0 | 0/0/0 |
| `tests1.dat` | 112/0/0 | 0/0/0 | 0/0/0 |
| `tests10.dat` | 54/0/0 | 0/0/0 | 0/0/0 |
| `tests11.dat` | 13/0/0 | 0/0/0 | 0/0/0 |
| `tests12.dat` | 2/0/0 | 0/0/0 | 0/0/0 |
| `tests14.dat` | 7/0/0 | 0/0/0 | 0/0/0 |
| `tests15.dat` | 14/0/0 | 0/0/0 | 0/0/0 |
| `tests16.dat` | 191/0/0 | 0/0/0 | 6/0/0 |
| `tests17.dat` | 13/0/0 | 0/0/0 | 0/0/0 |
| `tests18.dat` | 36/0/0 | 0/0/0 | 0/0/0 |
| `tests19.dat` | 103/0/0 | 0/0/0 | 0/0/0 |
| `tests2.dat` | 63/0/0 | 0/0/0 | 0/0/0 |
| `tests20.dat` | 64/0/0 | 0/0/0 | 0/0/0 |
| `tests21.dat` | 23/0/0 | 0/0/0 | 0/0/0 |
| `tests22.dat` | 5/0/0 | 0/0/0 | 0/0/0 |
| `tests23.dat` | 5/0/0 | 0/0/0 | 0/0/0 |
| `tests24.dat` | 8/0/0 | 0/0/0 | 0/0/0 |
| `tests25.dat` | 26/0/0 | 0/0/0 | 0/0/0 |
| `tests26.dat` | 20/0/0 | 0/0/0 | 0/0/0 |
| `tests3.dat` | 24/0/0 | 0/0/0 | 0/0/0 |
| `tests4.dat` | 0/0/0 | 9/0/0 | 0/0/0 |
| `tests5.dat` | 16/0/0 | 0/0/0 | 1/0/0 |
| `tests6.dat` | 39/0/0 | 13/0/0 | 0/0/0 |
| `tests7.dat` | 33/0/0 | 1/0/0 | 0/0/0 |
| `tests8.dat` | 10/0/0 | 0/0/0 | 0/0/0 |
| `tests9.dat` | 27/0/0 | 0/0/0 | 0/0/0 |
| `tests_innerHTML_1.dat` | 0/0/0 | 81/0/0 | 0/0/0 |
| `tricky01.dat` | 9/0/0 | 0/0/0 | 0/0/0 |
| `void-in-phrasing.dat` | 13/0/0 | 0/0/0 | 0/0/0 |
| `webkit01.dat` | 52/0/0 | 0/0/0 | 0/0/0 |
| `webkit02.dat` | 44/0/0 | 4/0/0 | 1/0/0 |
| **Total** | **1739/0/0** | **206/0/0** | **8/0/0** |

The original fixtures and their [source and license details](../tests/upstream/README.md) are preserved. The older focused fixtures in `tests/fixtures` also remain unchanged. These results cover the listed upstream cases, not every possible HTML input or every browser behavior.

### Upstream parse-error results

The tokenizer error inventory compares the original html5lib error lists with `Tokenizer::new_legacy_html5lib`, including exact code, order, line, and column. All 7,028 representable cases pass; four unpaired-surrogate cases are unrepresentable. The WPT tree fixture inventory compares complete ordered parse-error lists from both phases, including positions, rather than just the presence of an error. It uses a historical WPT name only when its meaning matches a Phos code. A case with prose-only positions or a historical code with no equivalent is unrepresentable; the presence of a separate `#new-errors` block does not by itself exclude a machine-readable legacy list. Remaining failures are actual list or position mismatches. Columns below are pass/fail/unrepresentable.

| html5lib fixture | Token errors P/F/U |
| --- | ---: |
| `contentModelFlags.test` | 24/0/0 |
| `domjs.test` | 59/0/0 |
| `entities.test` | 80/0/0 |
| `escapeFlag.test` | 9/0/0 |
| `namedEntities.test` | 4210/0/0 |
| `numericEntities.test` | 336/0/0 |
| `pendingSpecChanges.test` | 1/0/0 |
| `test1.test` | 69/0/0 |
| `test2.test` | 45/0/0 |
| `test3.test` | 1786/0/0 |
| `test4.test` | 85/0/0 |
| `unicodeChars.test` | 323/0/0 |
| `unicodeCharsProblematic.test` | 1/0/4 |
| **Total** | **7028/0/4** |

| WPT fixture | Document errors P/F/U | Fragment errors P/F/U | Script-on errors P/F/U |
| --- | ---: | ---: | ---: |
| `adoption01.dat` | 9/2/6 | 0/1/0 | 0/0/0 |
| `adoption02.dat` | 1/1/2 | 0/0/0 | 0/0/0 |
| `blocks.dat` | 48/0/0 | 0/0/0 | 0/0/0 |
| `comments01.dat` | 8/0/8 | 0/0/0 | 0/0/0 |
| `doctype01.dat` | 2/0/35 | 0/0/0 | 0/0/0 |
| `domjs-unsafe.dat` | 31/1/17 | 0/0/0 | 0/0/0 |
| `entities01.dat` | 16/2/57 | 0/0/0 | 0/0/0 |
| `entities02.dat` | 18/7/1 | 0/0/0 | 0/0/0 |
| `foreign-fragment.dat` | 0/0/0 | 25/0/41 | 0/0/0 |
| `html5test-com.dat` | 16/7/7 | 0/0/0 | 0/0/0 |
| `inbody01.dat` | 0/0/4 | 0/0/0 | 0/0/0 |
| `isindex.dat` | 4/0/0 | 0/0/0 | 0/0/0 |
| `main-element.dat` | 2/0/1 | 0/0/0 | 0/0/0 |
| `math.dat` | 0/0/0 | 0/0/8 | 0/0/0 |
| `menuitem-element.dat` | 0/0/20 | 0/0/0 | 0/0/0 |
| `namespace-sensitivity.dat` | 0/0/1 | 0/0/0 | 0/0/0 |
| `noscript01.dat` | 0/0/18 | 0/0/0 | 0/0/0 |
| `pending-spec-changes-plain-text-unsafe.dat` | 0/0/1 | 0/0/0 | 0/0/0 |
| `pending-spec-changes.dat` | 0/0/3 | 0/0/0 | 0/0/0 |
| `plain-text-unsafe.dat` | 3/0/34 | 0/11/0 | 0/0/0 |
| `processing-instructions.dat` | 0/124/0 | 0/0/0 | 0/0/0 |
| `quirks01.dat` | 0/0/4 | 0/0/0 | 0/0/0 |
| `ruby.dat` | 0/17/4 | 0/0/0 | 0/0/0 |
| `scriptdata01.dat` | 15/1/10 | 0/0/0 | 0/0/0 |
| `search-element.dat` | 2/0/1 | 0/0/0 | 0/0/0 |
| `svg.dat` | 0/0/0 | 0/0/8 | 0/0/0 |
| `tables01.dat` | 6/2/11 | 0/0/0 | 0/0/0 |
| `template.dat` | 1/1/121 | 1/1/2 | 0/0/0 |
| `tests1.dat` | 42/14/56 | 0/0/0 | 0/0/0 |
| `tests10.dat` | 11/0/43 | 0/0/0 | 0/0/0 |
| `tests11.dat` | 13/0/0 | 0/0/0 | 0/0/0 |
| `tests12.dat` | 1/1/0 | 0/0/0 | 0/0/0 |
| `tests14.dat` | 4/0/3 | 0/0/0 | 0/0/0 |
| `tests15.dat` | 1/0/13 | 0/0/0 | 0/0/0 |
| `tests16.dat` | 41/82/68 | 0/0/0 | 4/0/2 |
| `tests17.dat` | 1/0/12 | 0/0/0 | 0/0/0 |
| `tests18.dat` | 9/2/25 | 0/0/0 | 0/0/0 |
| `tests19.dat` | 29/12/62 | 0/0/0 | 0/0/0 |
| `tests2.dat` | 20/12/31 | 0/0/0 | 0/0/0 |
| `tests20.dat` | 36/24/4 | 0/0/0 | 0/0/0 |
| `tests21.dat` | 13/0/10 | 0/0/0 | 0/0/0 |
| `tests22.dat` | 2/3/0 | 0/0/0 | 0/0/0 |
| `tests23.dat` | 0/0/5 | 0/0/0 | 0/0/0 |
| `tests24.dat` | 8/0/0 | 0/0/0 | 0/0/0 |
| `tests25.dat` | 17/0/9 | 0/0/0 | 0/0/0 |
| `tests26.dat` | 0/7/13 | 0/0/0 | 0/0/0 |
| `tests3.dat` | 12/7/5 | 0/0/0 | 0/0/0 |
| `tests4.dat` | 0/0/0 | 9/0/0 | 0/0/0 |
| `tests5.dat` | 11/0/5 | 0/0/0 | 0/0/1 |
| `tests6.dat` | 9/7/23 | 0/0/13 | 0/0/0 |
| `tests7.dat` | 13/2/18 | 0/0/1 | 0/0/0 |
| `tests8.dat` | 2/1/7 | 0/0/0 | 0/0/0 |
| `tests9.dat` | 11/0/16 | 0/0/0 | 0/0/0 |
| `tests_innerHTML_1.dat` | 0/0/0 | 7/2/72 | 0/0/0 |
| `tricky01.dat` | 1/0/8 | 0/0/0 | 0/0/0 |
| `void-in-phrasing.dat` | 13/0/0 | 0/0/0 | 0/0/0 |
| `webkit01.dat` | 19/4/29 | 0/0/0 | 0/0/0 |
| `webkit02.dat` | 3/8/33 | 0/0/4 | 1/0/0 |
| **Total** | **524/351/864** | **42/15/149** | **5/0/3** |

## Development notes

Phos reports exact tokenizer errors for the representable pinned html5lib cases. Tree diagnostics now cover recovery across insertion modes, foreign content, templates, adoption agency, reprocessing, EOF, and fragments, but the WPT inventory above still contains exact-list and position mismatches and historical expectations that cannot be compared one-to-one. The byte frontend supports a single in-tree late-meta restart with original bytes; it does not implement statistical or locale-sensitive fallback detection, and its documented fallback is UTF-8. The fragment API receives one context element, so it cannot infer a `form` ancestor. The `&str` tokenizer cannot represent unpaired UTF-16 surrogates. Phos does not execute scripts or provide live DOM behavior such as form association and dynamic `selectedcontent` updates. Full HTML conformance has not been established. The next parser step is to reconcile the remaining representable WPT error-list mismatches by individual recovery branch and source position.

The next Phos milestone adds bounded CSS declaration scanning, explicit author cascade order and importance, a block/inline/inline-block box model, deterministic bundled-font metrics, rounded backgrounds and borders, clipping, and bounded PNG/JPEG image loading. The [rendering matrix, fixture inventory, examples, and remaining gaps](RENDERING.md) describe the deliberate subset. Precise international text shaping and an interactive viewport remain future work.

## Repository map

| Path | Purpose |
| --- | --- |
| `src/html/tokenizer.rs`, `src/html/named_references.rs`, `src/html/encoding.rs`, `src/html/errors.rs`, `src/html.rs` | HTML bytes, tokenization, diagnostics, and tree construction |
| `src/dom.rs` | Document arena and relationships |
| `src/network.rs`, `src/url.rs` | HTTP(S) loading and URL resolution |
| `src/css.rs`, `src/style.rs` | CSS parsing and computed styles |
| `src/layout.rs`, `src/paint.rs`, `src/text.rs` | Layout scene, font metrics, and SVG output |
| `src/resource.rs` | PNG/JPEG validation and limits |
| `tests/render`, `tests/render_fixtures.rs` | Pinned static-page rendering fixtures |
| `src/main.rs` | `scarlite` CLI |
| `examples/welcome.html` | Small sample document |
| `assets/logo.webp` | Scarlite logo |
| `tests/upstream` | Pinned upstream parser corpora |
| `tools/generate_named_references.py` | Regenerate the WHATWG named-reference table |

Licensed under Apache 2.0. See [LICENSE](../LICENSE).
