"""Turn an app icon's symbol into a free-standing pixel-art object, like Bedrock's gem:
no tile, a dark outline, light from the top left, violet-leaning shade on the bottom right.

    python scripts/pixel_object.py <flat_icon.png> <out.png> [grid]

The symbol is cut out of the icon's tile (everything that isn't the tile color), scaled to fill
the grid, then shaded. Works on light and dark backgrounds alike, so one file serves both.
"""
import random
import sys

from PIL import Image

src, out = sys.argv[1], sys.argv[2]
N = int(sys.argv[3]) if len(sys.argv) > 3 else 32
SCALE = 1024 // N

im = Image.open(src).convert("RGBA")
W = im.width
px = im.load()
# Tile color: most common opaque color.
counts = {}
for y in range(0, W, 4):
    for x in range(0, W, 4):
        r, g, b, a = px[x, y]
        if a > 200:
            k = (r // 8, g // 8, b // 8)
            counts[k] = counts.get(k, 0) + 1
tile = tuple(c * 8 + 4 for c in max(counts, key=counts.get))

# Cut the symbol out: pixels far from the tile color.
cut = Image.new("RGBA", im.size, (0, 0, 0, 0))
cp = cut.load()
for y in range(W):
    for x in range(W):
        r, g, b, a = px[x, y]
        if a > 140 and abs(r - tile[0]) + abs(g - tile[1]) + abs(b - tile[2]) > 60:
            cp[x, y] = (r, g, b, 255)
bbox = cut.getbbox()
sym = cut.crop(bbox)
# Fit into the grid with a 2-cell margin for the outline, keeping proportions.
inner = N - 4
sw, sh = sym.size
f = inner / max(sw, sh)
sym = sym.resize((max(1, round(sw * f * 8)), max(1, round(sh * f * 8))), Image.Resampling.LANCZOS)
canvas = Image.new("RGBA", (N * 8, N * 8), (0, 0, 0, 0))
canvas.alpha_composite(sym, ((N * 8 - sym.width) // 2, (N * 8 - sym.height) // 2))
small = canvas.resize((N, N), Image.Resampling.BOX)
sp = small.load()

kind = [[1 if sp[x, y][3] > 110 else 0 for x in range(N)] for y in range(N)]


def at(x, y):
    return kind[y][x] if 0 <= x < N and 0 <= y < N else 0


def lum(c):
    return 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]


def shade(c, f):
    if f >= 1:
        return tuple(min(255, round(v + (255 - v) * (f - 1))) for v in c)
    return tuple(max(0, round(v * f)) for v in c)


cells = [(x, y) for y in range(N) for x in range(N) if kind[y][x]]
cx = sum(p[0] for p in cells) / len(cells)
cy = sum(p[1] for p in cells) / len(cells)
rng = random.Random(7)
OUTLINE = (16, 14, 22)
o_img = Image.new("RGBA", (N, N), (0, 0, 0, 0))
o = o_img.load()
for y in range(N):
    for x in range(N):
        if kind[y][x]:
            r, g, b, _ = sp[x, y]
            base = (r, g, b)
            if lum(base) > 225:
                base = (226, 224, 232)
            lit = at(x, y - 1) == 0 or at(x - 1, y) == 0
            dark = at(x, y + 1) == 0 or at(x + 1, y) == 0
            f = 1.22 if lit and not dark else 0.62 if dark and not lit else 0.92 if lit and dark else 1.0
            if (x - cx) + (y - cy) > 1:
                f *= 0.78
            f *= 1 + rng.choice([-0.06, 0, 0, 0.05])
            c = shade(base, f)
            if f < 0.9:
                c = (round(c[0] * 0.9), round(c[1] * 0.86), min(255, round(c[2] * 1.02)))
            o[x, y] = tuple(min(255, round(v / 12) * 12) for v in c) + (255,)
        elif any(at(x + dx, y + dy) for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))):
            o[x, y] = OUTLINE + (255,)
o_img.resize((N * SCALE, N * SCALE), Image.Resampling.NEAREST).save(out)
print("wrote", out)
