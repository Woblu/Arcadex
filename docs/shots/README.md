# Screenshots

The site loads `.webp`, not the PNGs you take. Drop a full-size PNG in here
with the right name, run the conversion below, and commit only the `.webp` it
produces — the PNGs are gitignored because they are several megabytes each and
nothing serves them.

| PNG you drop in | What the site loads | Width | Where it goes |
| --- | --- | --- | --- |
| `library.png` | `library.webp` | 2048 | The big one under the hero |
| `skin-arcadex.png` | `skin-arcadex.webp` | 1600 | Layouts, first card |
| `skin-console.png` | `skin-console.webp` | 1600 | Layouts, second card |
| `skin-bigpicture.png` | `skin-bigpicture.webp` | 1600 | Layouts, third card |
| `skin-arc.png` | `skin-arc.webp` | 1600 | Layouts, fourth card |
| `skin-browser.png` | `skin-browser.webp` | 1600 | Layouts, fifth card |
| `skin-channels.png` | `skin-channels.webp` | 1600 | Layouts, sixth card |

Anything missing keeps its drawn placeholder, so adding one at a time is fine.
The page probes each file before swapping it in, which also means a `.webp`
that fails to load degrades to the drawing rather than to a broken image.

## Converting

From `docs/`, with Pillow installed:

```python
from PIL import Image
for src, w in [('shots/library.png', 2048),
               ('shots/skin-arcadex.png', 1600),
               ('shots/skin-console.png', 1600),
               ('shots/skin-bigpicture.png', 1600),
               ('shots/skin-arc.png', 1600),
               ('shots/skin-browser.png', 1600),
               ('shots/skin-channels.png', 1600)]:
    im = Image.open(src).convert('RGB')
    im.resize((w, round(im.height * w / im.width)), Image.LANCZOS) \
      .save(src.replace('.png', '.webp'), 'WEBP', quality=82, method=6)
```

Quality 82 was chosen against these shots: box art stays clean and the four
files come to around 310 KB together, against 10.8 MB as PNGs. The
layout shots are wider than the cards need because clicking one opens it
full size.

`og.jpg` one level up is the social preview card, 1200×630, cropped from
`library.png`. Regenerate it whenever the hero screenshot changes, since link
previews are the one image that has to be a JPEG.

## Taking them

- Use a library with enough games to fill the grid. A screenshot of four games
  looks like a screenshot of four games.
- Let the metadata finish first so the covers are actually there.
- Dark background, no desktop behind it.
- The three layout shots should be of the same library, so they read as one app
  in three shapes rather than three different programs.

One thing worth knowing: box art in a screenshot is publisher artwork. Normal
enough for a project like this, and everyone does it, but it is not ours — so
keep it to screenshots rather than using cover art as decoration elsewhere.
