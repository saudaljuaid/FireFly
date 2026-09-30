# Phos font assets

`DejaVuSans.ttf` and `DejaVuSans-Bold.ttf` are the existing DejaVu faces.
Their Bitstream Vera license (DejaVu changes are public domain) is preserved
in `LICENSE-dejavu.txt`. The TTFs are unmodified.

`PhosCjk-Regular.otf` is a renamed, reduced derivative of Noto Sans CJK SC
Regular, version 2.004. Copyright 2014–2021 Adobe
(https://www.adobe.com/), licensed under SIL Open Font License 1.1; the complete
license is preserved in `LICENSE-noto-cjk.txt` and in the font metadata.
It contains **10,805 Unicode mappings** and is **2,826,132 bytes**, versus the
16,437,364-byte source. Its SHA-256 is
`1195d8195d8fbe19ccd2cc716e6383ee56a28165a4d5a7d5224d4798253ffc43`.

Pinned upstream source:

- Repository: https://github.com/notofonts/noto-cjk
- Commit: `f8d157532fbfaeda587e826d4cd5b21a49186f7c`
- File: `Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Regular.otf`
- Download: https://raw.githubusercontent.com/notofonts/noto-cjk/f8d157532fbfaeda587e826d4cd5b21a49186f7c/Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Regular.otf
- Source SHA-256: `2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b`

Reproduce with Python and **fonttools 4.60.2**:

```sh
python -m pip install fonttools==4.60.2
python assets/fonts/subset_cjk.py /path/to/NotoSansCJKsc-Regular.otf assets/fonts/PhosCjk-Regular.otf
```

The script keeps complete GB2312 and JIS X0208 Chinese/Japanese repertoires,
CJK punctuation, kana, kana extensions, fullwidth forms, and U+FFFD. It
omits vertical and regional GSUB alternate glyphs and renames both OpenType
name records and CFF names. Glyphs use the source's Simplified Chinese forms;
this is not language-specific Japanese or Traditional Chinese typography.
The repertoire is independent of Phos fixtures. Korean Hangul, uncommon
Han characters, emoji, and many other scripts are outside this face's
coverage. A missing glyph is rendered using the selected DejaVu face's
visible U+FFFD glyph, with an explicit missing-glyph count in the run.

The engine emits deduplicated vector glyph outlines from these faces, not
font binaries. The SVG remains self-contained, and no installed font is
used for visible text. OFL's document exception applies to rendered SVGs;
redistributing the font asset itself retains the OFL notice and license.
