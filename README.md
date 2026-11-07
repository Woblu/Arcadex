# Arcadex

[arcadex website](https://woblu.github.io/Arcadex/) · [download](https://github.com/Woblu/Arcadex/releases/latest)

A ROM library manager and emulator launcher. Point it at the folders where you
keep your ROMs. It identifies each one, pulls down cover art and details, and
launches it through RetroArch or a standalone emulator. Playtime is tracked per
game.

Tauri 2 (Rust) + React + TypeScript, with SQLite for the library.

## Scope

Arcadex indexes ROMs you already have on disk. It doesn't search for or
download games. ScreenScraper and TheGamesDB are metadata databases: they serve
artwork and descriptions for games they can identify, not the games themselves.

ROM hacks are a different thing and are supported. A patch contains only the
hack author's own changes; the base ROM comes from you.

## Coming from Playdex

Arcadex was called Playdex up to 0.15. The first time Arcadex starts, it moves
your library into its own data folder: the database with your playtime and
favourites, artwork, imported patches, disc playlists and the unpack cache.
Saved passwords move across in the system's credential store the same way.
Nothing is copied twice and nothing is deleted.

| | Playdex kept it in | Arcadex keeps it in |
| --- | --- | --- |
| Windows | `%APPDATA%\com.boazv.playdex` | `%APPDATA%\com.boazv.arcadex` |
| macOS | `~/Library/Application Support/com.boazv.playdex` | `~/Library/Application Support/com.boazv.arcadex` |
| Linux | `~/.local/share/com.boazv.playdex` | `~/.local/share/com.boazv.arcadex` |

If you want a copy first, close Playdex and copy `library.db` out of the old
folder before starting Arcadex.

On Windows the update installs Arcadex beside Playdex rather than over it,
because the installer is named after the app. Once Arcadex has opened your
library, uninstall Playdex from **Settings → Apps**. Starting Playdex again
after the move shows an empty library: nothing is lost, it is all in Arcadex.

## What it does

### Scanning

Every library folder is walked for known ROM extensions. Each file gets CRC32,
MD5 and SHA1 in a single pass.

For a `.zip` or `.7z`, the ROM *inside* is hashed, not the archive. That's what
the metadata databases and the No-Intro/Redump DATs index, so hashing the
container matches nothing while still looking like a real dump hash. Archive
headers list every entry and its size, so choosing the right one costs a header
read and only that entry gets decompressed. `.rar` stays opaque.

Existing entries are normally left alone. Two exceptions:

- An entry indexed under a rule that's since been fixed (unresolved platform,
  or an archive that was never opened) gets re-detected and re-hashed in place.
  The scan summary calls these *corrected*. Titles, artwork and any platform
  you set by hand survive.
- An entry the scanner now recognises as a mistake gets *dropped*, so a fixed
  heuristic cleans up after itself. The file on disk is never touched. Anything
  you've played, favourited, patched or used as a hack's base game is exempt.

A file dropped onto the window usually lives outside every library folder, so
walking the folders never reaches it. A scan looks at those again as well, so
a game added that way before a rule was fixed is put right like the rest.

An archive too large to hash still records which file inside it is the game.
Without that it looked unopened to every scan after, and was read again and
reported as corrected each time.

### Getting started

A card in the corner lists the three things between installing and playing:
adding your games, giving each system something to run it, and (optionally)
an account for descriptions. It says which systems cannot start yet, by name,
and its buttons go straight to where each is set up. It goes away once every
system you have can start, or when hidden.

### Adding games

Drag a ROM onto the window and it joins the library. The whole window is the
target, since someone dragging a file at a program is not aiming at anything in
particular.

Dropped files are indexed where they lie rather than copied anywhere. It is
your ROM in your folder, and quietly duplicating gigabytes into the app data
directory to make the bookkeeping tidier is not a trade worth making. Drop a
*folder* and it becomes a library folder, then gets scanned, which is plainly
what dropping a folder means.

Either way it runs the same indexing a scan does: same platform detection, same
rejection rules, same hashing, same cleaned-up title, and the same automatic
metadata fetch afterwards. A second "quick add" path would be a second place
for those rules to drift.

### Discs kept as folders

A disc game that has not been zipped is usually a folder holding a playlist and
its tracks: `Final Fantasy VII.cue` beside three or four `.bin` files. That is
one game, and the cue is the part an emulator wants, so that is the part
Arcadex indexes. Tracks named by a playlist in the same folder are skipped, as
is any leftover track-shaped file sitting beside one, since dumps get renamed
and a stray `.bin` next to a cue sheet was never a second game.

Playlists are also exempt from the size floor. A cue sheet is a couple of
hundred bytes, and rejecting it for being small threw away the only usable file
in the folder while keeping the tracks it spoke for.

A playlist says nothing about itself, so its system comes from the track it
points at: Saturn, Dreamcast and Mega CD all write their name into the start of
the data track. Where that fails - a PlayStation disc, say - the folder's
assigned system in **Settings → ROM folders** settles it for everything
underneath.

### Consoles that install their games

From the PlayStation 3 on, a dump is usually the installed game rather than a
file: a PlayStation 3 disc copied off is `PS3_GAME/USRDIR/EBOOT.BIN` and a few
thousand data files, plenty of them called something `.bin`. Walked file by
file, that was one game called "EBOOT" and hundreds more called whatever the
data was named.

These folders are now recognised whole, by files the console itself put there,
and each one is a single game:

| System | Folder | Title from |
|---|---|---|
| PlayStation 3 | `PS3_GAME/PARAM.SFO` (a disc), or `PARAM.SFO` and `USRDIR` (installed) | `PARAM.SFO` |
| PlayStation 4 | `eboot.bin` beside `sce_sys/param.sfo` | `param.sfo` |
| PlayStation 5 | `eboot.bin` beside `sce_sys/param.json` | `param.json` |
| Wii U | `code/*.rpx` and `meta/meta.xml` | `meta.xml` |
| Xbox 360 | `default.xex` at the top | the folder's name |

The entry points at the file the emulator boots, takes its title from the
console's own metadata and its size from the whole folder, and nothing beneath
it is walked. A PlayStation 3 update or a PlayStation 4 patch is claimed the
same way, so its files stay out of the library, but it is not listed, since
there is nothing to start. A PlayStation Vita dump has the same shape as a
PlayStation 4 one and is told apart by its `PARAM.SFO`. The structure outranks
a folder's assigned system: `PS3_GAME` is not a guess.

Nothing in one of these is hashed. The boot file is a few megabytes of
executable whose checksum matches no catalogue, and the game around it runs to
tens of gigabytes.

A library scanned before this listed those data files one by one. The next
scan drops them, says so in its summary, and gives the `EBOOT.BIN` its real
system and title back. As everywhere else, anything you have played or
favourited is left alone.

Discs and packages are identified by reading them:

- **PlayStation 3 `.iso`**: `PS3_DISC.SFB` in the ISO 9660 root, which sits
  in the part of the disc that is never encrypted.
- **Xbox and Xbox 360 `.iso`**: neither uses ISO 9660 for the game. Both use
  XDVDFS, whose volume descriptor is 32 sectors into the game partition. A
  full dump keeps a video partition in front, at an offset that depends on
  the disc generation, so each known offset is tried. The root then says
  which console: `default.xbe` boots an Xbox, `default.xex` an Xbox 360.
- **Xbox 360 Games on Demand and Arcade packages** have no extension at all.
  They are found by the folder they are filed in (`00007000`, `000D0000`)
  and read by their header, which carries the dashboard's own title for the
  game.

Switch dumps (`.nsp`, `.xci`, `.nsz`, `.xcz`) and Wii U single files (`.wua`,
`.wud`, `.wux`) need none of this, since their extensions are their own.

Multi-disc games on these systems are not folded into an `.m3u`. Their
emulators do not read one, and Xenia swaps an Xbox 360 game's discs itself.

**Kept in a zip or 7z.** Any of these layouts archived whole is recognised
from the archive's list of names - a package file in its `00007000` folder, a
`PS3_GAME` folder, `sce_sys` beside `eboot.bin` - and named from the one small
file inside that says what it is, without unpacking the rest. A zipped Games on
Demand package is a 45 KB header and several gigabytes of data, and only the
header is read. The entry keeps the archive as its file, with the size of
everything inside it. Playing one unpacks the whole archive (see Launching).

**Not covered.** No emulator runs Xbox One games, so there is nothing to
point one at. PlayStation 5 emulation is at an early stage and runs a small
number of smaller titles, which is why that system is listed: the library can
hold the games, and launching one runs whatever emulator you set. PlayStation
Vita games are installed into Vita3K rather than started from a path, and
`.pkg` files are installers on every PlayStation, so neither is indexed.

### Telling ROMs from everything else

ROM folders fill up with manuals, box scans, BIOS dumps and installers, and
extensions lie. A `.bin` is as likely to be firmware or a CD audio track as a
game. Every candidate is checked before it's hashed:

- Magic numbers for images, PDFs, Office documents, executables and media
- BIOS and firmware filename patterns
- A 1 KB size floor
- Positive header confirmation for NES, Game Boy, GBA, N64 and Mega Drive

Emulators end up inside ROM libraries all the time, and an emulator is a folder
full of files that look like ROMs. Dolphin's `Sys` directory alone holds a dozen
`.bin` files of firmware, fonts and cheat databases. So any directory containing
a program is skipped whole rather than walked. Your own library folders are
never skipped, only directories underneath them. Archives get the same
treatment: one containing an executable is an application, not a game dump.

The bias is toward keeping files. Only definite evidence rejects one. A header
mismatch is flagged *suspect* and indexed anyway, since losing a real game is
worse than listing a stray file. The scan summary says what was thrown out and
why.

### Platform detection

`.bin`, `.cue`, `.iso`, `.chd` and `.zip` are shared across a dozen systems, so
extensions alone don't settle it. Arcadex works through this order:

1. The platform assigned to the library folder, if you set one
2. An extension only one system uses (`.sfc`, `.gba`, `.nes`)
3. For an archive, an unambiguous extension among its entries
4. **The file's own bytes** — a magic number in its first kilobyte
5. A directory name matching a system alias (`.../Sega Genesis/...`)
6. The ROM's own filename, and the names inside the archive
7. Failing all that, the first candidate system, which you can correct from the
   game's detail panel

Sony discs are the exception to all of that: nothing in a PlayStation or PSP
header says what it is. They are identified by reading the disc's own
filesystem instead — the ISO 9660 volume descriptor at sector 16, the root
directory it points at, and the one small file in there that answers the
question. `SYSTEM.CNF` names the boot executable under `BOOT2` on a
PlayStation 2 and `BOOT` on a PlayStation; a PSP disc carries `UMD_DATA.BIN`
and `PSP_GAME`. Three short reads, wherever the image is on disk. This only
works for images with 2048-byte sectors, which is what `.iso` means — a raw
`.bin` track wraps every sector in error correction and is a different
problem.

Step 4 is the only one that isn't a guess. Extensions and filenames are what
somebody typed; a magic number is what the machine wrote. It exists for the
ambiguous cases: a `.iso` could be six different systems, but a disc image
announces which one it is in its header. A Wii dump carries `5D1C9EA3` at
offset 0x18, or at 0x218 when a WBFS container wraps it; a GameCube disc has
`C2339F3D` at 0x1C; a Switch NSP opens with `PFS0`; Saturn, Dreamcast and Mega
CD each name themselves in plain text. Only the first kilobyte is read.

Because it runs after the extension has had its say, a signature can only
settle a file that was going to be guessed at anyway — it can never overrule a
`.sfc`. Archives skip it, since reaching a header inside one means
decompressing it and their inner extension has already spoken at step 3.

Step 5 matters more than it sounds. Dumps are routinely named "Mario Kart Wii"
or "Sonic (Mega Drive)", and for a container extension like `.7z` that belongs
to no system at all, the filename is the last real evidence available. It
matches whole words only. The looser substring rule used for directory names
reads "Legbreaker" as Game Boy, which is fine for a folder someone named after
a system and useless for a game title.

Extensions that genuinely belong to two systems stay ambiguous. Dolphin's
`.rvz`, `.gcz` and `.wia` each hold either a GameCube or a Wii disc, so they
fall through to the steps above instead of filing every Wii dump under
GameCube.

### Artwork and metadata

libretro-thumbnails (RetroArch's own artwork server) is the default source for
box art, screenshots and title screens. No account, no API key, no quota. The
URL is the query, built from the system name and the ROM's No-Intro filename,
so it works as soon as a scan finishes. It serves images only, which is why the
credentialed providers below still earn their keep.

The system-name table in `scrape/libretro.rs` was copied from the
libretro-thumbnails repository listing rather than guessed. A wrong folder name
there is a silent 404.

Finish a scan and metadata fetches automatically for anything new. Only entries
that still need it get looked up, and the progress toast can cancel it. Games
already marked *notfound* are left alone too: asking again for something both
providers have already said they do not have spends quota for nothing.

**Settings → Metadata → Fetch everything again** goes back over the whole
library instead, including games that already have artwork, and replaces what
it finds. It is the way to pick up art that has been added to a provider since
you last looked, or to redo a library scraped with worse credentials. It is a
separate query rather than a flag on the automatic pass, so the pass after a
scan can never start spending quota by accident. On a large library it takes a
while and uses a good deal of a daily ScreenScraper allowance, which is why it
lives in Settings rather than next to Scan.

ScreenScraper goes first, keyed on the ROM's hash, so a correct dump matches
exactly instead of by fuzzy name. If it has no match or its quota is spent,
TheGamesDB is tried by name: results are scored against the cleaned-up title,
boosted when the platform agrees, and a weak match is discarded rather than
guessed at. Both providers report which system they matched, which fills in a
platform Arcadex couldn't work out on its own. A platform already on the record
is left alone since you may have set it by hand.

### Artwork you supply yourself

Some games will never be found. A ROM hack, an obscure dump, somebody's
homebrew — no amount of refetching produces a cover for something no provider
has a record of. **Choose an image** in a game's own panel opens a file picker
and takes any image the webview can display: PNG, JPEG, WebP, GIF, BMP or AVIF.

The file is copied into that game's media folder rather than referenced where
it sits. Artwork is already handled this way while ROMs are not, and for the
same reason: the library owns its artwork, and pointing at something in a
Downloads folder means the cover vanishes the next time that folder is tidied.

A cover chosen by hand outranks anything fetched afterwards. `apply_metadata`
skips the cover column when `cover_custom` is set, so **Fetch everything
again** cannot quietly undo the work of picking artwork by hand — while
everything else in that fetch still lands. **Use fetched artwork** gives the
choice up again.

### Renaming, and borrowing another game's details

**Rename** in a game's panel changes its title. Like a hand-picked cover, a name
typed by hand is marked, and `apply_metadata` leaves a marked title alone — so a
later fetch that matches the game cannot put the provider's name back over the
one you chose. Rescans never touched titles to begin with.

**Copy details from another game** is for the games no provider knows because
they are a change to one it does. A hack of Super Mario 64 can take Super Mario
64's description, developer, genre, release date and artwork while keeping its
own name. The artwork files are copied into the receiving game's own media
folder rather than shared: removing a game deletes its media folder, so sharing
paths would leave the hack's art pointing at nothing the moment the original
was removed. The borrowed cover is protected from later fetches the same way a
chosen one is.

In the Console appearance a game's panel opens on **right-click** as well as
from a pad's X button and the footer's Options button, since the tile is the
thing a mouse user points at.

Artwork downloads once into the app data directory and is served to the UI over
a private `media://` protocol. It serves images and nothing else: anything in
the `media` folder, and the thumbnail RetroArch writes beside a save state. The
rest of the app data directory, the library database included, is out of its
reach.

The UI runs under a content security policy that allows only its own scripts,
images from itself and that protocol, and no outside network at all. Every
download happens in Rust. A description or title a metadata provider sends is
data shown as text, and the policy means that even a bug that let one be read
as markup would have nothing to load and nowhere to send anything.

### Verifying dumps

A DAT is a catalogue: for every known-good dump of a system, its exact size and
checksums. Import one under **Settings → Verification** and every game's panel
says whether its checksums match a catalogued dump, whether nothing in the
catalogue has them, or whether it was too large to have been hashed at all.

This is what all the hashing was for. Arcadex already computes CRC32, MD5 and
SHA-1 for everything in one pass, and hashes the ROM *inside* an archive rather
than the archive, which is exactly what a DAT indexes — so checking is close to
a database join.

SHA-1 is tried first, then MD5, then CRC32. CRC32 comes last because it is
short enough that a collision is conceivable, and a DAT is meant to settle
arguments rather than start them.

The DATs are not shipped or fetched. No-Intro (cartridges) and Redump (discs)
publish through a click-through page with no API, so the files come from you.
That is also why both work equally well here, where a mirror would have covered
one and not the other.

"Not in the catalogue" is not the same as "bad". It means no DAT you have
loaded lists those checksums, which covers bad dumps and modified ROMs, but
also any system you simply have not imported a DAT for.

A scan does not hash anything over 512 MB, since reading every disc image in a
library would turn a scan into an afternoon. That leaves most disc games with
nothing to verify, so a game's panel offers **Checksum this file**: one file,
read once, when you ask. The checksums it stores are the same ones a scan would
have taken, so the game can then be verified against a DAT and matched by
checksum on ScreenScraper the next time it is fetched. A console game kept as a
folder has no single file to checksum; its serial (`BLUS30443`, read from its
own metadata) is shown instead, and sent to ScreenScraper in place of one.

### ROM hacks

Point Arcadex at an IPS, UPS or BPS patch and it patches a *copy* of a ROM in
your library. The original is never modified. The result is added as its own
game with its own title, art, saves and playtime.

UPS and BPS embed the CRC32 of the ROM they were built against, and Arcadex
already hashes everything during scanning, so a mismatch is caught before
patching rather than producing a corrupt game:

```
this patch expects CRC32 X, yours is Y
most likely a different revision or region
```

The patch formats live in `patch.rs` and are unit-tested.

Patches can also be imported in bulk, either from a folder or straight from a
`.7z` bundle. Bundles are streamed and only the patch files are extracted, so a
35 GB archive costs a read instead of 35 GB of disk. One downloaded from the
Internet Archive is deleted once its patches are out, and only that archive's
own address is accepted for download. Entry paths that try to
escape the destination are refused. Each patch is indexed by the CRC32 of the
ROM it targets, so a game's detail panel can list exactly which catalogued hacks
its dump will run.

### Launching

RetroArch is invoked as `retroarch -L <core> <rom>`. Each system has a preferred
libretro core which you can override, or you can point a system at a standalone
emulator using a command template with `{rom}`. The detail panel shows the exact
command before you click Play, so a wrong path is obvious. When the emulator
exits, the session length is added to that game's playtime.

Archives get unpacked first, but only when they have to be. RetroArch ships an
`.info` file next to every core listing the extensions it accepts, and no core
lists `zip` or `7z`. RetroArch handles archives itself in the frontend and never
passes one to a core, so that list says nothing about archives at all.

What it does say is whether the core loads discs, and disc cores are exactly the
ones RetroArch will not unpack for. Dolphin's list reads:

```
gcm|iso|wbfs|ciso|gcz|elf|dol|dff|tgc|wad|rvz|m3u|wia
```

Those are disc formats, so Dolphin wants a path to a real `.wbfs` and a Wii game
kept as a `.7z` gets unpacked once. A NES or N64 ROM in a `.zip` is left alone,
because RetroArch opens it itself. With no core to ask (a standalone emulator, or
a RetroArch that cannot be found) the ROM is unpacked, since a real file always
works.

A console game that is more than one file - an Xbox 360 package and its data,
a PlayStation 3 folder - is unpacked whole, since the one entry that looks most
like a ROM is only part of the game, and the emulator is handed the file it
starts from. It goes into a `.partial` folder renamed only once every file is
out, so an unpack cut short is never taken for a finished one.

Unpacking happens off the main thread and with the library unlocked, so the
window keeps working for the minute or two a disc or a package takes, and a
card shows how far it has got. The game starts when it is done.

Unpacked ROMs are keyed by the archive's checksum and reused, so this costs one
extraction rather than one per launch. **Settings → Emulators** shows what the
cache is using, sets its size limit (64 GB by default, since one disc game can
be most of a small limit on its own) and can empty it. Past the limit the game
you have not played in longest is dropped. The originals are never touched, and
anything cleared is rebuilt on the next launch. The game about to start is
never evicted, even when it alone is larger than the limit.

Keeping both copies is the price of storing a disc game compressed, so a game's
detail panel offers to collapse them: **Unpack and keep only the game** moves the
unpacked file out of the cache into the folder the archive lived in, repoints
the library entry, and then deletes the archive. That order is the design — the
replacement is in place and checked before anything is removed, and moving it
out of the cache puts it beyond the reach of eviction, which would otherwise be
free to delete the only remaining copy. It is the one thing in Arcadex that
deletes a file of yours, and it only happens when asked for directly.

For an archived console game it keeps the whole game rather than one file. An
archive holding one folder unpacks as that folder beside it; one holding loose
files gets a folder named after the archive, rather than spilling them into the
folder it sat in. If a launch already unpacked it, the copy in the cache is
moved instead of unpacked a second time. Every byte is checked to have arrived
before the archive is deleted.

### Cheats

Switching a cheat on is the whole interaction. Pressing Play writes the enabled
codes into RetroArch's cheat folder, under the *core's* display name, which is
where RetroArch actually looks. If the game has cheats enabled it also turns on
RetroArch's "Auto-Apply Cheats During Game Load", since a cheat file RetroArch
never reads does nothing.

Launch is the right moment for both, because RetroArch definitely isn't running
and can't overwrite its own config on exit. **Write now** exists only for
prepping a ROM you plan to start from RetroArch directly.

### Saves

Save files and save states are listed together, with RetroArch's state
thumbnails where it wrote them. Arcadex looks in the configured save
directories, the per-core subfolders RetroArch creates when sorting is on, and
the ROM's own folder. Back up copies everything into a dated folder. Only save
states can be deleted; battery saves hold real progress and are refused.

### Sync between machines

**Settings → Sync** keeps saves and play time the same on every machine you
play on, through a folder you already sync: Syncthing, OneDrive, Dropbox or a
NAS share. There is no server and Arcadex uploads nothing; it writes into the
folder and whatever syncs the folder carries it across.

- **Play time.** Each machine keeps a journal of its own sessions in the
  folder and reads everyone else's at startup, or when you press **Sync play
  time now**. A session is only ever counted once, and one for a game this
  machine does not have yet is picked up once the game is added.
- **Saves.** Before a game starts, newer saves from another machine are
  brought in; this machine's are backed up first. After it closes, its saves
  go into the folder if they changed. Each machine keeps its last ten
  versions there.
- **Both changed?** The newer wins. The other is kept, as a backup here or a
  version in the folder, so nothing is lost either way.

Games are matched by what they are rather than where they are: the same dump
is the same game on every machine, even under another file name or zipped on
one and not the other. Saves follow RetroArch games. Standalone emulators keep
theirs in their own memory cards and folders, which can be pointed inside the
sync folder in each emulator.

Every file in the folder has exactly one machine writing it, because sync
tools handle two machines changing the same file badly. The layout:

```text
Arcadex/
  machines/<machine>.json             who each machine is, and when it last synced
  play/<machine>.jsonl                every session played on that machine
  saves/<system>/<game>/<machine>.json   the newest saves that machine has
  saves/<system>/<game>/versions/        the files, one folder per version
```

### Skins

Six layouts, under **Settings → Appearance**:

- **Arcadex**: the desktop view. Sidebar, sortable grid, detail panel.
- **Console**: a home screen. One row of large icons sized against the window,
  a caption underneath, and a row of small round system buttons that each go
  somewhere different, including an "All software" grid.
- **Lounge**: living room. Hero art, left rail, capsule shelf. (It was called
  Big Picture, which is Valve's name for Steam's own TV mode.)
- **Arc**: one column of slabs down the middle of a dark screen, lit from one
  side. The selected slab steps toward the light rather than only changing
  colour, which is what stops the column reading as a list of links.
- **Browser**: a column of cards standing in a slow-drifting field. Thin
  letter-spaced type and a lot of space doing nothing, on purpose.
- **Channels**: the one light layout. A fixed 4x3 page of framed channels you
  move between with arrows rather than scrolling, a soft blue glow on the one
  you are on, and a silver tray along the bottom with the clock in the middle.
  A half-full page draws the empty sockets rather than simply ending early,
  which is the detail that makes the shape read.

They're separate component trees, not a palette swap, since a console home
screen is a different shape rather than a different colour. All state and
fetching stays in `App`, so they can't drift apart in behaviour. Anything a skin
doesn't draw itself (cheats, saves, ROM hacks) opens the shared detail panel, so
no feature exists in one skin and not another.

Registering one is two entries — the component in `SHELLS` in `App.tsx` and its
row in `SKINS` in `skins/shell.ts`. Nothing branches on the name: the body class
and the settings list are both derived from `SKINS`, and the panel styling comes
from re-pointing design tokens rather than restyling each control.

Nothing is traced from another product's assets, and no skin is named after the
hardware it nods at. The shapes are the obvious ones for each context, drawn in
CSS here.

### Controller

A pad is picked up on its own, with no pairing step.

| Button | Does |
|---|---|
| D-pad / left stick | Move focus |
| A | Press what's focused, or start the game on a game tile |
| B | Close, or step back |
| X | Open a game's panel |
| Y | Favourite |
| LB / RB | Previous / next system |
| Start | Play the selection |

Back steps out of wherever you are before it closes anything: the console
skin's "All software" grid and its search box are views inside the skin, so
Back leaves those first. Escape does the same thing, so there is always a way
out without a pad.

Movement is geometric rather than a hand-written map of what sits next to what.
Everything ordinarily interactive is a candidate, and the nearest thing in the
direction you pushed wins, weighted so travel along the axis beats drift across
it. A new button or a whole new skin is navigable the moment it renders, with no
second structure to keep in sync. Settings gets controller navigation for free.

A pad the browser does not recognise often reports its D-pad as a single hat
axis instead of four buttons: eight positions, a seventh of the axis apart,
clockwise from up. Those are decoded position by position. (They used to be
read as a fraction of a turn with the test the wrong way round, which sent
every press on such a pad in the opposite direction.)

The focus ring thickens while a pad is connected, since it has to be visible
from a sofa.

### Stats

Playtime is recorded per session. The library view shows total time, session
count, longest session, and jump-back-in and most-played lists that open the
game.

### Updates

Arcadex checks for a new version a few seconds after launch, once the library
has drawn, so a slow or unreachable endpoint never delays startup. If it finds
one it says so in a corner card with the release notes. Nothing downloads until
you accept.

A failed check is silent, since being offline is normal for a desktop app. The
**Check for updates** button under **Settings → Appearance → About** reports the
real reason when you ask it directly.

Every update is verified against a public key compiled into the app before any
of it runs, so a tampered or mis-hosted file is refused rather than installed.

## Windows may delete the installer

Windows Defender sometimes removes Arcadex, usually reporting
**PUA:Win32/Packunwan** or a machine-learning name like *Trojan:Win32/Wacatac*.
It is a false positive, and it comes from three things about the build rather
than anything it does:

- The installer is an NSIS self-extractor using solid LZMA compression. To a
  heuristic looking for packers, a small executable that unpacks a much larger
  one is the shape of a packed binary.
- It is unsigned. There is no Authenticode certificate on it, so there is no
  publisher for Windows to attribute it to.
- Every release is a brand new file almost nobody has downloaded, so it has no
  reputation with SmartScreen. Reputation resets on every version.

None of that is fixed by asking Defender nicely. What actually helps, in order:

1. **Install from the `.msi` instead of the `-setup.exe`.** Both are in every
   release and install the same application. The MSI is a Windows Installer
   database rather than a compressed self-extractor, so the packer heuristic
   has nothing to fire on.
2. **Report it to Microsoft** at
   <https://www.microsoft.com/en-us/wdsi/filesubmission>, as a software
   developer, marked "incorrectly detected". These are normally corrected
   within a few days, and it fixes it for everyone rather than one machine.
3. **Sign the installers.** The real fix, and free for this project. See
   below.

### Signing it, for free

[SignPath Foundation](https://signpath.org/) issues code signing certificates
to open source projects at no cost, and signs through their own service rather
than handing over a key: the private key stays in their HSM and the CI run
submits the file to be signed. Their
[conditions](https://signpath.org/terms.html) are that the project has no
malware or bundled unwanted software, uses an OSI-approved licence with no
commercial dual-licensing and no proprietary components, is actively
maintained, is already released in the form being signed, and describes what it
does on its download page. The team doing the signing has to be the team that
maintains the code and owns the repository.

Arcadex meets all of that, but SignPath also looks for a project people
already use, and turns down new ones until they have an audience. So this
waits until Arcadex has one. Once a project is accepted, signing becomes a
step in `release.yml` that submits the built installers and gets signed ones
back.

The alternatives, for completeness:

- **Azure Trusted Signing**, about ten dollars a month. Cheap rather than free,
  and needs identity validation.
- **Certum's open source certificate**, roughly thirty euros a year, on a
  hardware token they post to you.
- **A self-signed certificate is not an option.** It is free, and it does
  nothing here — Windows only trusts a certificate chaining to a root it
  already knows, so a self-signed build is exactly as untrusted as an unsigned
  one, with the added downside of looking like it is trying to appear signed.

Until then, every release carries a **build provenance attestation**: a signed
statement that the files came from this repository, at a named commit, out of a
named workflow run. It does not make Windows trust anything — only Authenticode
does — but it turns "this is really our build" from something you have to take
on trust into something anyone can check:

```bash
gh attestation verify Arcadex_x.y.z_x64_en-US.msi --repo Woblu/Arcadex
```

To check a download is the real thing rather than trusting this page, compare
it against the release:

```powershell
Get-FileHash .\Arcadex_x.y.z_x64_en-US.msi -Algorithm SHA256
```

Every installer is built in public by GitHub Actions from the commit the tag
points at — the run, its logs and the resulting file are all on the releases
page — so the hash you compute should match the file you downloaded from there
byte for byte.

## macOS says Arcadex is damaged

The Mac build is signed ad hoc, not by a registered Apple developer, and is
not notarised. That takes a paid Apple Developer account, which this project
does not have yet. A downloaded app in that state is refused the first time
with a message that it "is damaged and can't be opened". It is not damaged;
macOS is saying it cannot tell who made it.

After dragging Arcadex into Applications, either:

- open **System Settings → Privacy & Security**, scroll to the message about
  Arcadex and choose **Open Anyway**, or
- run this once in Terminal, which removes the download flag macOS put on it:

  ```bash
  xattr -dr com.apple.quarantine /Applications/Arcadex.app
  ```

This is only needed for the first install. Updates are downloaded by Arcadex
itself rather than a browser, are checked against its own update key, and do
not carry the flag.

## Setup

```bash
npm install
npm run tauri dev      # development
npm run tauri build    # installer / bundled app

npm test               # frontend tests
cd src-tauri && cargo test
```

`.github/workflows/test.yml` runs both on every push: the frontend once, the
Rust tests on Linux and Windows.

Needs Rust, and the platform's webview toolchain:

- **Windows**: the MSVC build tools plus the WebView2 runtime (Windows 11 ships
  with it).
- **macOS**: Xcode command line tools. WebKit is part of the system.
- **Linux**: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libxdo-dev`,
  `libssl-dev`, `librsvg2-dev`, `libappindicator3-dev` and `patchelf`. The
  release workflow installs exactly this list, so it is the one to copy.

Releases are built for all three. `.github/workflows/release.yml` runs one job
per platform against a draft release and publishes it only once every one of
them has finished, so a release never ships missing somebody's installer.

### Credentials

Both providers are optional. Artwork works with no credentials at all through
libretro-thumbnails; these two add the text metadata it can't supply. Enter them
under **Settings → Metadata**.

The passwords and the API key go to the system's credential store: the
Credential Manager on Windows, the Keychain on macOS, the Secret Service on
Linux. The library database is a file that gets copied into backups and synced
folders along with everything else, which is no place for a password. Where no
store is available, such as a Linux session with no Secret Service running,
they stay in the database as before rather than failing to save. A library from
an earlier version moves them over on its next start.

On macOS, an unsigned build is a new program as far as the Keychain is
concerned each time it updates, so the first fetch after an update may ask
whether Arcadex can use the saved password. **Always Allow** answers it until
the next one.

**ScreenScraper** (`screenscraper.fr`) needs two logins, which people often
confuse:

- A *developer* ID and password, requested from the site once you have an
  account. Nothing is bundled with this app, and without a dev key the API
  answers only a trickle of requests.
- Your *personal* ScreenScraper login, which raises the per-minute and daily
  quota. Contributors get a higher allowance.

**TheGamesDB** (`thegamesdb.net`) is a free API key with a monthly allowance,
used as the fallback.

**Test credentials** saves what you've entered and asks both providers whether
the keys work, reporting your remaining allowance, so a bad key shows up
immediately instead of as a wall of failures mid-scrape.

Scraping paces itself at roughly three requests a second. A provider that says
its allowance is spent - ScreenScraper's commonest refusal is per minute - sits
out for a minute while the others carry on, and the run only stops when there
is nothing left to ask; the summary says which provider rested. A game that
fails on its own, artwork that will not save say, is marked as an error and the
run moves on to the next.

While a fetch runs, the rest of the app is not held up: Play, Favourite and
Rename work as normal, and the progress card has its own Stop button.

### Emulators

**Settings → Emulators** tries to find RetroArch for you. It checks the standard
install folders, Steam (via the registry, then following `libraryfolders.vdf`
across every drive, since Steam is often not on C:), every drive root, Scoop,
Chocolatey, the macOS app bundle, and Linux package and Flatpak paths, then
falls back to `PATH`.

Detection runs automatically the first time you open the tab with nothing set,
and **Detect RetroArch** re-runs it. It reports where it found the install and
how many cores are there, so a wrong hit is obvious. The cores folder is
inferred from the executable if you leave it blank. Paths are stripped of the
quotes Explorer's "Copy as path" adds, both on save and on use.

**Find emulators** does the same for standalone emulators. It looks in the
places people unzip them - a folder called Emulators on any drive, the drive
roots, Program Files, Downloads, Scoop, EmuDeck and ES-DE on Windows, and the
Desktop, Documents and Downloads wherever the system says they are, which with
OneDrive backup on is inside OneDrive rather than the user folder; the
Applications folders on macOS; package paths, Flatpak exports and AppImages in
`~/Applications` on Linux - and recognises each by its executable's name. A
system you have games for that can only run on a standalone emulator, and has
none set, is pointed at the first one found for it; anything already set is
left alone, and the others are listed with a button to use them. It runs by
itself when you open the tab with such a system unset.

Recognising by name means telling an emulator from the programs that come with
it: `DolphinTool`, `XeniaManager`, shadPS4's Qt launcher, `yuzu-cmd`. Those
are not mistaken for the emulator.

Per system you can override the core or switch to a standalone emulator:

```
"C:\Emulators\Dolphin\Dolphin.exe" -b -e "{rom}"
```

Systems with no libretro core in existence, the Switch for one, carry an empty
core list. Launching one tells you to point it at a standalone emulator instead
of blaming your RetroArch setup.

**Browse** next to a standalone command recognises the emulator from its
executable's name and fills in what it needs, which for some of them is more
than the ROM path:

| Emulator | Command |
|---|---|
| RPCS3 | `--no-gui "{rom}"`, or it opens its game list instead |
| shadPS4 | `-g "{rom}"` (the emulator, not its separate Qt launcher) |
| Xenia | `"{rom}"` |
| xemu | `-dvd_path "{rom}"` |
| Cemu | `-g "{rom}"` |
| Eden, Citron and the other yuzu descendants | `-g "{rom}"` |
| Ryujinx | `"{rom}"` |
| PCSX2, DuckStation | `-batch "{rom}"`, so it exits when the game does and play time stops counting |
| Dolphin | `-b -e "{rom}"` |

Anything else gets `"emulator" "{rom}"`. Either way the command can be edited
after, and is saved when you leave the field rather than on every key. `{rom}` is filled in after the command is split into arguments, so a
path with spaces is one argument whether or not the template quotes it.

Arcadex does not download or link to emulators, firmware or keys. RPCS3 needs
the PlayStation 3 system software installed, xemu needs files dumped from an
Xbox, and the Switch emulators need keys and firmware from your own console;
each emulator's own setup guide covers that.

## Releasing

Updates are published as GitHub releases and found through the endpoint in
`plugins.updater.endpoints`. Pushing a version tag is the whole thing:

```bash
# bump `version` in package.json, src-tauri/tauri.conf.json
# and src-tauri/Cargo.toml so all three agree
git commit -am "Console skin, controller navigation, in-app updates"
git tag v0.2.0
git push && git push --tags
```

`.github/workflows/release.yml` builds the installer, signs it, and uploads it
with a `latest.json` manifest. **The commit message becomes the release notes**,
which is what the in-app notice shows, so write it for whoever reads it.

The repository has to be public. The updater fetches
`releases/latest/download/latest.json` with no credentials; on a private repo
that's a 404 and every check fails silently.

### The signing key

`tauri signer generate` produced a keypair. The public half is in
`tauri.conf.json`. The private half lives outside this repository at
`~/.playdex/updater.key`, in a folder that ignores itself so a stray
`git add -A` can't pick it up.

- **Anyone holding the private key can sign an update Arcadex will trust and
  install.** Keep it in a password manager and in GitHub repository secrets,
  nowhere else.
- **Losing it can't be undone.** Installed copies only accept updates signed by
  the matching key, so a new keypair means replacing every existing install by
  hand. Back it up now, not later.

CI needs it as two secrets under *Settings → Secrets and variables → Actions*:
`TAURI_SIGNING_PRIVATE_KEY` (the file's contents) and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (empty unless you set one).

For a signed build locally:

```bash
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.playdex/updater.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
npm run tauri build
```

Without those, the build still produces an installer but stops before the `.sig`
files and says so. An unsigned release is one no existing copy can install.

## Layout

```
src-tauri/src/
  lib.rs          app setup, media:// protocol, command registration
  secrets.rs      passwords in the system credential store
  commands.rs     every command the UI can call
  db.rs           SQLite schema and queries
  scan.rs         folder walking, platform detection, title cleanup
  hashing.rs      one-pass CRC32/MD5/SHA1, reads into zip and 7z
  platforms.rs    system table: extensions, aliases, preferred cores
  romcheck.rs     telling ROMs from manuals, BIOS dumps and box art
  dats.rs         No-Intro / Redump catalogues and dump verification
  signature.rs    identifying a system from magic numbers in the header,
                  ISO 9660 and XDVDFS disc filesystems
  consoles.rs     games that are a folder: PARAM.SFO, param.json, meta.xml,
                  Xbox 360 packages
  emulators.rs    standalone emulators: finding them, and the arguments
                  each one needs
  scrape/
    mod.rs        provider orchestration and fallback
    libretro.rs   artwork, no credentials needed
    screenscraper.rs
    thegamesdb.rs
  patch.rs        IPS/UPS/BPS patching with CRC verification
  hacks.rs        patch catalog import (folders and 7z bundles)
  detect.rs       finding an installed RetroArch
  cheats.rs       Game Genie codes and RetroArch's cheat format
  saves.rs        save files and save states
  media.rs        artwork download and cache
  launch.rs       command construction, archive extraction, playtime

src/
  App.tsx         state, event wiring, controller handling
  api.ts          typed wrappers over the Tauri commands
  gamepad.ts      pad polling and geometric focus movement
  update.ts       update check, download, install
  skins/          the six layouts, and what they're all handed
  components/     grid, detail panel, settings, toasts, modals
    settings/     one component per Settings tab
    detail/       the game panel's longer sections
  *.test.ts       frontend tests, run with `npm test`
```

## Where hacks come from

ROMhacking.net went read-only in 2024 and released its database and file archive
to the Internet Archive, where it lives as the *ROM Hack Patch Archive*: 43
files, around 35 GB, packaged as per-system 7z bundles rather than a queryable
catalog. That's why importing is a one-time local operation instead of a live
API. The active successor community is romhack.ing.

Patches only ever carry the hack author's own changes. The base ROM comes from
you.

## Ideas

- **Save state and screenshot browsing** per game.

## Notes

- Removing a game removes the library entry only, and asks first. The file on
  disk is never touched. Hacks made from it stay in the library as games of
  their own; a multi-disc set goes with its discs. The single exception is
  **Unpack and keep only the game**, which deletes an archive after replacing
  it with the game from inside, and says so before it does.
- **Clean missing entries** counts what it would remove and asks before
  removing it. A game on a drive or network share that is not connected is not
  missing, and is left alone.
- The library lives in the app data directory next to the artwork cache, so
  deleting that folder resets everything without touching your ROMs.

## What Arcadex will not do

Arcadex manages games you already have. It does not search for, download,
host or link to games, BIOS files, firmware, decryption keys or the tools
that dump them, and it never will.

- **Emulators are found, not fetched.** Find emulators looks for what you
  have installed and fills in how to run it. It does not download emulators
  or say where to get them.
- **Firmware and keys are yours to provide.** Several emulators need system
  software or keys dumped from a console you own. Each emulator's own setup
  guide covers that; Arcadex does not.
- **Artwork and descriptions belong to their owners.** They are fetched at
  run time from ScreenScraper, TheGamesDB and libretro-thumbnails, for games
  already in your library, and kept on your machine.
- **Not affiliated with any console maker.** Console and company names are
  used only to say which system a game is for; they are trademarks of their
  owners. The app's icons are its own line drawings, not anyone's logo.

See [SECURITY.md](SECURITY.md) to report a security problem, and
[CONTRIBUTING.md](CONTRIBUTING.md) to help.

## License

MIT. See [LICENSE](LICENSE).

Arcadex bundles no games, no BIOS files and no DAT catalogues. Artwork and
descriptions are fetched at runtime from third-party metadata providers and
belong to their respective owners.
