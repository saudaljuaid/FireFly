# Upstream HTML parser fixtures

The files in `tree/` are byte-for-byte copies of all 58 non-script-executing
`.dat` files in `html/syntax/parsing/resources/` at web-platform-tests/wpt
commit `7a8d143bce12c1142109d27a8de4b0b26f9f5a47`. This includes the
three files whose names end in `unsafe.dat`; that label concerns the browser
test harness, and the cases are useful for Phos's standalone parser. The four
`scripted_*.dat` files require running JavaScript during parsing and are not
included. The upstream license is reproduced at `tests/wpt-LICENSE`.

The files in `tokenizer/` are byte-for-byte copies of all 13 files with a
`tests` array in `html5lib/html5lib-tests/tokenizer/` at commit
`224991ec10db04f056a89eed8b0bd8695fd2950e`. `xmlViolation.test` has a
separate `xmlViolationTests` array for XML compatibility behavior; it is not
part of the HTML tokenizer corpus. The upstream license is reproduced at
`tests/html5lib-LICENSE`.

Inputs and expected outputs are not modified. The Rust harness interprets
html5lib's `doubleEscaped` strings before testing. Four cases containing
unpaired UTF-16 surrogates cannot be passed to Phos's UTF-8 `&str` API and
are counted as unrepresentable, not passing. The tree tests compare complete
serialized trees exactly, and also check arena ownership and parent/child
integrity for every case. `tests/upstream_errors.rs` inventories tokenizer
errors exactly and compares the representable WPT error lists. WPT cases with
prose-only positions or historical names without an equivalent Phos code are
reported as unrepresentable rather than silently treated as passing. A
`#new-errors` block does not exclude a comparable legacy `#errors` list.
The original fixture files remain untouched.
