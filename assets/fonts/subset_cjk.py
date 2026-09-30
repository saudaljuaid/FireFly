"""Reproduce PhosCjk-Regular.otf with fonttools==4.60.2.

Download the source stated in README.md, then run:
python subset_cjk.py NotoSansCJKsc-Regular.otf PhosCjk-Regular.otf
The source download is deliberately not stored in the repository.
"""

import hashlib
import sys

from fontTools import subset
from fontTools.ttLib import TTFont

SOURCE_SHA256 = "2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b"


def repertoire():
    # Complete standard legacy Chinese/Japanese repertoires, rather than a
    # hand-picked set of characters used in the rendering fixtures.
    chars = set(range(0x20, 0x7F))
    for codec in ("gb2312", "euc_jp"):
        for lead in range(0xA1, 0xFF):
            for trail in range(0xA1, 0xFF):
                try:
                    chars.update(map(ord, bytes((lead, trail)).decode(codec)))
                except UnicodeDecodeError:
                    pass
    for start, end in ((0x3000, 0x3100), (0x31F0, 0x3200), (0xFF00, 0xFFF0)):
        chars.update(range(start, end))
    chars.add(0xFFFD)
    return sorted(chars)


def main():
    source, output = sys.argv[1:]
    with open(source, "rb") as source_file:
        digest = hashlib.sha256(source_file.read()).hexdigest()
    if digest != SOURCE_SHA256:
        raise SystemExit("Source font SHA-256 does not match the pinned font")
    font = TTFont(source, recalcTimestamp=False)
    options = subset.Options()
    options.recalc_timestamp = False
    options.canonical_order = True
    options.name_IDs = [0, 1, 2, 3, 4, 5, 6, 13, 14, 16, 17]
    options.name_legacy = True
    options.name_languages = [0x409]
    # Static horizontal SC glyph forms. Omit vertical and regional GSUB
    # alternates; Latin shaping continues to use the complete DejaVu faces.
    options.layout_features = []
    subsetter = subset.Subsetter(options=options)
    subsetter.populate(unicodes=repertoire())
    subsetter.subset(font)
    for record in font["name"].names:
        names = {
            1: "Phos CJK", 2: "Regular", 3: "Phos CJK Regular 1.0",
            4: "Phos CJK Regular", 6: "PhosCjk-Regular", 16: "Phos CJK", 17: "Regular",
        }
        if record.nameID in names:
            record.string = names[record.nameID].encode(record.getEncoding())
    if "CFF " in font:
        cff = font["CFF "].cff
        cff.fontNames = ["PhosCjk-Regular"]
        cff.topDictIndex[0].FamilyName = "Phos CJK"
        cff.topDictIndex[0].FullName = "Phos CJK Regular"
    font.save(output, reorderTables=True)
    print(f"Kept {len(font.getBestCmap())} Unicode mappings")


if __name__ == "__main__":
    main()
