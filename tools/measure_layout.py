"""Measure original bounded layout cases through the actual scarlite command.

Sources, SVGs, and timing JSON must be saved outside the checkout. Standard
library only; this is an engineering measurement script, not a UI application.
"""
import argparse
import json
from pathlib import Path
import statistics
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--binary", type=Path, required=True)
parser.add_argument("--output-dir", type=Path, required=True)
parser.add_argument("--samples", type=int, default=3)
parser.add_argument("--width", type=int, default=640)
args = parser.parse_args()
if not 1 <= args.samples <= 9 or not 1 <= args.width <= 16384:
    parser.error("samples must be 1..9 and width 1..16384")
repo = Path(__file__).resolve().parent.parent
outputs = args.output_dir.resolve()
if outputs == repo or repo in outputs.parents:
    parser.error("output-dir must be outside the repository")
binary = args.binary.resolve(strict=True)
outputs.mkdir(parents=True, exist_ok=True)
directory = outputs / "sources"
directory.mkdir(exist_ok=True)
base = '<!doctype html><meta charset=utf-8><style>html,body{margin:0}body{font-size:16px;line-height:24px}p{margin:0 0 8px}section{padding:12px}</style>'
cases = {
    'text-heavy': base + '<section>' + ('<p>The geometry of this paragraph combines unequal labels, combining e\u0301, CJK 中文, and direction العربية with Latin labels.</p>' * 150) + '</section>',
    'many-flex': base + '<style>section{display:flex;flex-wrap:wrap;gap:8px}.item{flex:1 1 100px;min-width:0;padding:4px;border:1px solid}</style><section>' + ''.join(f'<div class=item>Control {i}: archive metadata</div>' for i in range(1200)) + '</section>',
    'many-grid': base + '<style>section{display:grid;grid-template-columns:repeat(6,minmax(0,1fr));gap:8px}.item{padding:4px;border:1px solid}</style><section>' + ''.join(f'<div class=item>Panel {i}: status summary</div>' for i in range(1200)) + '</section>',
    'deep-nested': base + '<style>.flex{display:flex;flex-direction:column}.grid{display:grid;grid-template-columns:1fr}div{padding:1px}</style>' + ''.join(f'<div class={"flex" if i % 2 else "grid"}>' for i in range(64)) + 'Nested content with retained glyph runs.' + '</div>' * 64,
    'long-token': base + '<section>' + 'A' * 100_000 + '</section>',
    'large-repeat-span': base + '<style>section{display:grid;grid-template-columns:repeat(100000,1fr)}div{grid-column:1/span 100000}</style><section><div>Bounded hostile tracks</div></section>',
    'malformed-css': base + '<style>' + 'div{width:calc(1px + bogus);color:rgb(1,2,);grid-template-columns:repeat(-2,1fr);}' * 5000 + 'div{width:100px;height:20px}</style><div>Later valid declarations</div>',
}
records = []
for name, source in cases.items():
    path = directory / f'{name}.html'
    path.write_text(source, encoding='utf-8')
    svg = directory / f'{name}.svg'
    timings = []
    warnings = []
    for _ in range(args.samples):
        start = time.perf_counter()
        result = subprocess.run([str(binary), str(path), '--width', str(args.width), '--output', str(svg)], capture_output=True, text=True, timeout=60)
        timings.append((time.perf_counter() - start) * 1000)
        if result.returncode:
            raise RuntimeError(result.stderr)
        warnings = result.stderr.splitlines()
    rendered = svg.read_text(encoding='utf-8')
    record = {'case': name, 'input_bytes': len(source.encode()), 'viewport_width': args.width, 'process_ms': timings, 'median_ms': statistics.median(timings), 'svg_bytes': svg.stat().st_size, 'truncated': rendered.startswith('<svg data-phos-truncated="true"'), 'warnings': warnings}
    records.append(record)
    print(json.dumps(record), flush=True)
(outputs / "performance.json").write_text(json.dumps({
    "binary": str(binary), "profile": "process samples include startup and SVG writing; record compiler/profile/environment separately",
    "samples": args.samples, "records": records,
}, indent=2), encoding="utf-8")
