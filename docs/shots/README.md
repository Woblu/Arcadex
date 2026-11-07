# Screenshots

The site loads `.webp`, not the PNGs you take. Put a full-size PNG in here
with the right name, run the conversion below, and commit only the `.webp` it
produces. The PNGs are gitignored because they are several megabytes each and
nothing serves them.

| PNG you drop in | What the site loads | Width | Where it goes |
| --- | --- | --- | --- |
| `library.png` | `library.webp` | 2048 | The big one under the hero (the Console layout) |
| `skin-arcadex.png` | `skin-arcadex.webp` | 1600 | Layouts, first card |
| `skin-console.png` | `skin-console.webp` | 1600 | Layouts, second card |
| `skin-lounge.png` | `skin-lounge.webp` | 1600 | Layouts, third card |
| `skin-arc.png` | `skin-arc.webp` | 1600 | Layouts, fourth card |
| `skin-browser.png` | `skin-browser.webp` | 1600 | Layouts, fifth card |
| `skin-channels.png` | `skin-channels.webp` | 1600 | Layouts, sixth card |

Anything missing keeps its drawn placeholder, so adding one at a time is fine.
The page probes each file before swapping it in, which also means a `.webp`
that fails to load degrades to the drawing rather than to a broken image.

## Taking them

`shoot.mjs` takes all six from a running build, through WebView2's DevTools
port, at 1600×856 rendered at twice the resolution. So they come out the same
size, framing and selection every time, with no window frame and nothing from
the desktop behind.

1. Build a copy under its own identifier, so it never touches a real library:

   ```bash
   echo '{"identifier": "com.boazv.arcadex.verify"}' > verify.json
   npx tauri build --no-bundle --config verify.json
   ```

2. Give it a library in `%APPDATA%\com.boazv.arcadex.verify`: enough games
   with their artwork already fetched to fill a row, with the Getting started
   card dismissed. Give the games plain paths like `D:\Games\N64\...`: the
   desktop layout's panel shows the file's path, and a real one has your user
   name in it.

3. Start it with the DevTools port open, and run the script:

   ```bash
   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9333" \
     src-tauri/target/release/arcadex.exe &
   node docs/shots/shoot.mjs 9333 docs/shots
   cp docs/shots/skin-console.png docs/shots/library.png
   ```

   Name layouts after the port and folder to retake only those:
   `node docs/shots/shoot.mjs 9333 docs/shots steam arc`.

The layouts should all be of the same library, so they read as one app in six
shapes rather than six different programs.

## Converting

From `docs/shots/`, with Pillow installed:

```python
from PIL import Image
for src, w in [('library.png', 2048),
               ('skin-arcadex.png', 1600),
               ('skin-console.png', 1600),
               ('skin-lounge.png', 1600),
               ('skin-arc.png', 1600),
               ('skin-browser.png', 1600),
               ('skin-channels.png', 1600)]:
    im = Image.open(src).convert('RGB')
    im.resize((w, round(im.height * w / im.width)), Image.LANCZOS) \
      .save(src.replace('.png', '.webp'), 'WEBP', quality=82, method=6)
```

Quality 82 keeps box art clean, and the seven files come to about 600 KB
together. The layout shots are wider than the cards need because clicking one
opens it full size.

## The social card

`og.jpg` one level up is the link preview: 1200×630, the wordmark and one line
beside the Console layout's row of covers. It has to be a JPEG, and it is the
one image people see before they visit, so remake it whenever the hero
changes or the name, the line or the platforms do.

## Box art

Box art in a screenshot is publisher artwork. Normal enough for a project like
this, and everyone does it, but it is not ours, so keep it to screenshots
rather than using cover art as decoration elsewhere.
