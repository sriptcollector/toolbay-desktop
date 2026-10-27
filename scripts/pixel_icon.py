"""Turn a flat app icon into Bedrock-style pixel art: a 32x32 grid, dark outline,
light from the top left, shade to the bottom right and a little texture.

    python scripts/pixel_icon.py <source.png> <out.png> [grid]

The source keeps its layout (tile shape, symbol, colors); only the rendering changes.
Then run `npx tauri icon <out.png>` to make every size.
"""
import random
import sys

from PIL import Image

src, out = sys.argv[1], sys.argv[2]
N = int(sys.argv[3]) if len(sys.argv) > 3 else 32
SCALE = 1024 // N

im = Image.open(src).convert("RGBA")
small = im.resize((N, N), Image.Resampling.BOX)
px = small.load()

# The tile color: the most common opaque color.
counts = {}
for y in range(N):
    for x in range(N):
        r, g, b, a = px[x, y]
        if a > 200:
            k = (r // 8, g // 8, b // 8)
            counts[k] = counts.get(k, 0) + 1
tk = max(counts, key=counts.get)
tile = tuple(c * 8 + 4 for c in tk)


def lum(c):
    return 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]


def dist(a, b):
    return sum(abs(a[i] - b[i]) for i in range(3))


# Classify: 0 empty, 1 tile, 2 symbol.
kind = [[0] * N for _ in range(N)]
for y in range(N):
    for x in range(N):
        r, g, b, a = px[x, y]
        if a < 140:
            continue
        kind[y][x] = 2 if dist((r, g, b), tile) > 60 else 1


def at(x, y):
    return kind[y][x] if 0 <= x < N and 0 <= y < N else 0


def shade(c, f):
    """f > 1 lightens toward white, f < 1 darkens."""
    if f >= 1:
        return tuple(min(255, round(v + (255 - v) * (f - 1))) for v in c)
    return tuple(max(0, round(v * f)) for v in c)


def quant(c, step=12):
    return tuple(min(255, round(v / step) * step) for v in c)


sym = [(x, y) for y in range(N) for x in range(N) if kind[y][x] == 2]
sym_center = (sum(p[0] for p in sym) / len(sym), sum(p[1] for p in sym) / len(sym))

rng = random.Random(7)
OUTLINE = (12, 11, 16)
outp = Image.new("RGBA", (N, N), (0, 0, 0, 0))
o = outp.load()
for y in range(N):
    for x in range(N):
        k = kind[y][x]
        if k == 0:
            continue
        noise = 1 + rng.choice([-0.06, 0, 0, 0.05])
        if k == 1:
            # Tile: outline on its outer edge, a lit rim top/left, a shaded rim bottom/right.
            if 0 in (at(x - 1, y), at(x + 1, y), at(x, y - 1), at(x, y + 1)):
                c = OUTLINE
            elif at(x, y - 2) == 0 or at(x - 2, y) == 0:
                c = shade(tile, 1.18)
            elif at(x, y + 2) == 0 or at(x + 2, y) == 0:
                c = shade(tile, 0.62)
            else:
                # Gentle top-left to bottom-right gradient across the tile.
                g = 1.10 - 0.22 * ((x + y) / (2 * N))
                c = shade(tile, g * noise)
            # A dark outline around the symbol, like Bedrock's edge.
            if c != OUTLINE and 2 in (at(x - 1, y), at(x + 1, y), at(x, y - 1), at(x, y + 1)):
                c = OUTLINE
        else:
            base = px[x, y][:3]
            # Pure white has no room to light; start a little below it, like Bedrock's pale stone.
            if lum(base) > 225:
                base = (226, 224, 232)
            # Facets: lit where the top/left neighbor isn't symbol, shaded where bottom/right isn't.
            lit = at(x, y - 1) != 2 or at(x - 1, y) != 2
            dark = at(x, y + 1) != 2 or at(x + 1, y) != 2
            f = 1.0
            if lit and not dark:
                f = 1.22
            elif dark and not lit:
                f = 0.62
            elif lit and dark:
                f = 0.92
            # Split light like Bedrock: the top-left half bright, the bottom-right half in shade.
            cx, cy = sym_center
            if (x - cx) + (y - cy) > 1:
                f *= 0.78
            c = shade(base, f * noise)
            if f * noise < 0.9:
                # Shadows lean violet, as in Bedrock.
                c = (round(c[0] * 0.9), round(c[1] * 0.86), min(255, round(c[2] * 1.02)))
        o[x, y] = quant(c) + (255,)

outp.resize((N * SCALE, N * SCALE), Image.Resampling.NEAREST).save(out)
print("wrote", out)
