"""Draw the Arcadex wordmark as pixel art: white letters, a dark outline one
pixel wide all round, and a shadow one pixel down and right.

    python docs/wordmark.py docs/arcadex-wordmark.svg
"""
import sys

ROWS = 10  # cap height; lowercase sits on rows 3-9

GLYPHS = {
    "A": [
        "..####..",
        ".######.",
        "##....##",
        "##....##",
        "##....##",
        "########",
        "########",
        "##....##",
        "##....##",
        "##....##",
    ],
    "r": [
        "##.###",
        "######",
        "###...",
        "##....",
        "##....",
        "##....",
        "##....",
    ],
    "c": [
        ".#####.",
        "#######",
        "##.....",
        "##.....",
        "##.....",
        "#######",
        ".#####.",
    ],
    "a": [
        ".#####.",
        ".######",
        ".....##",
        ".######",
        "##...##",
        "#######",
        ".######",
    ],
    "d": [
        ".....##",
        ".....##",
        ".....##",
        ".######",
        "#######",
        "##...##",
        "##...##",
        "##...##",
        "#######",
        ".######",
    ],
    "e": [
        ".#####.",
        "#######",
        "##...##",
        "#######",
        "##.....",
        "#######",
        ".#####.",
    ],
    "x": [
        "##...##",
        "###.###",
        ".#####.",
        "..###..",
        ".#####.",
        "###.###",
        "##...##",
    ],
}

word = "Arcadex"
pixels = set()
x0 = 2  # room for the outline
for ch in word:
    g = GLYPHS[ch]
    top = ROWS - len(g)
    for r, line in enumerate(g):
        for c, v in enumerate(line):
            if v == "#":
                pixels.add((x0 + c, 1 + top + r))
    x0 += len(g[0]) + 1

outline = {(x + dx, y + dy) for (x, y) in pixels for dx in (-1, 0, 1) for dy in (-1, 0, 1)}
shadow = {(x + 1, y + 1) for (x, y) in outline}
width = max(x for x, _ in shadow) + 1
height = max(y for _, y in shadow) + 1


def runs(cells):
    """Merge each row's pixels into horizontal runs, to keep the file small."""
    out = []
    for y in sorted({y for _, y in cells}):
        xs = sorted(x for x, yy in cells if yy == y)
        start = prev = xs[0]
        for x in xs[1:] + [None]:
            if x is not None and x == prev + 1:
                prev = x
                continue
            out.append((start, y, prev - start + 1))
            if x is not None:
                start = prev = x
    return out


def layer(cells, fill):
    rects = "".join(f'<rect x="{x}" y="{y}" width="{w}" height="1"/>' for x, y, w in runs(cells))
    return f'<g fill="{fill}">{rects}</g>'


svg = (
    f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" '
    f'width="{width * 6}" height="{height * 6}" shape-rendering="crispEdges" '
    f'role="img" aria-label="Arcadex">'
    + layer(shadow, "#0b0d11")
    + layer(outline, "#3a4048")
    + layer(pixels, "#ffffff")
    + "</svg>\n"
)
open(sys.argv[1], "w", encoding="utf-8").write(svg)
print(f"{width}x{height} units, ratio {width / height:.2f}")
