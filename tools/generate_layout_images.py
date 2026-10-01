"""Create original small engineering rasters; no third-party source assets."""
from pathlib import Path
import struct
import zlib

directory = Path(__file__).resolve().parent.parent / 'tests' / 'render'

def chunk(name, body):
    return struct.pack('>I', len(body)) + name + body + struct.pack('>I', zlib.crc32(name + body))

for name, width, height in [('landscape', 96, 64), ('portrait', 64, 96), ('wide', 128, 64)]:
    rows = []
    for y in range(height):
        row = bytearray([0])
        for x in range(width):
            color = (215, 225, 210)
            if 8 <= x < width - 8 and 6 <= y < height - 6:
                color = (247, 242, 222)
            if x == 18 and 10 <= y < height - 10:
                color = (170, 103, 84)
            if 23 <= x < width - 13 and y in [height // 3, height // 2, 2 * height // 3]:
                color = (91, 119, 111)
            if name == 'wide' and x in [width // 2 - 1, width // 2]:
                color = (178, 173, 148)
            row.extend(color)
        rows.append(row)
    data = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(b''.join(rows))) + chunk(b'IEND', b'')
    path = directory / f'layout-{name}.png'
    path.write_bytes(data)
    print(path, len(data), width, height)
