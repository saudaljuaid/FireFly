"""Regenerate Phos's named references from WHATWG's published table."""

import json
from pathlib import Path
from urllib.request import urlopen

SOURCE = "https://html.spec.whatwg.org/entities.json"
DESTINATION = Path(__file__).resolve().parents[1] / "src/html/named_references.rs"


def rust_string(text: str) -> str:
    result = '"'
    for char in text:
        codepoint = ord(char)
        if char == "\\":
            result += "\\\\"
        elif char == '"':
            result += '\\"'
        elif 32 <= codepoint <= 126:
            result += char
        else:
            result += f"\\u{{{codepoint:x}}}"
    return result + '"'


with urlopen(SOURCE, timeout=30) as response:
    source = json.load(response)

entries = sorted(
    (name[1:], "".join(chr(codepoint) for codepoint in value["codepoints"]))
    for name, value in source.items()
)
lines = [
    f"// Generated from {SOURCE} (WHATWG HTML Standard).",
    "// Names omit the leading ampersand; semicolonless legacy names remain distinct.",
    "pub(super) const NAMED_REFERENCES: &[(&str, &str)] = &[",
]
lines += [f"    ({rust_string(name)}, {rust_string(value)})," for name, value in entries]
lines += [
    "];",
    f"pub(super) const MAX_NAMED_REFERENCE_LENGTH: usize = {max(len(name) for name, _ in entries)};",
    "",
]
DESTINATION.write_text("\n".join(lines), encoding="utf-8")
print(f"Wrote {len(entries)} references to {DESTINATION}")
