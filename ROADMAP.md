# Roadmap

Things worth building, roughly in the order they'd help. Checked items
have shipped; the version they shipped in is in brackets, or "next" for
what is done but not yet released.

## Before the next release

What stands between the work already done and putting it in front of
people.

- [ ] **Run it against a real library.** Thousands of files, not the
      handful used while building this. The first run of 0.16 moves a
      Playdex library into Arcadex's folder, so back up `library.db` first.
- [ ] **Try the Flatpak on a Steam Deck.** Built by the release workflow and
      never yet run on one: Game Mode, a controller in WebKitGTK, RetroArch
      from Discover, and a ROM on the SD card.

## Waiting on someone else

- **Sign the Windows installers.** SignPath Foundation signs open source
  projects for free, but turns down new ones until they have users. Apply
  once Arcadex has some. Until then the `.msi`, Microsoft's false-positive
  form, and the provenance attestation are the answer. See the README.
- **Notarise the macOS build.** It needs a paid Apple Developer account.
  Builds are signed ad hoc meanwhile, and the README says how to open one.
- **Flathub.** Flathub builds from source, so this needs a manifest that
  builds Arcadex itself, with its Rust and npm dependencies vendored by
  `flatpak-cargo-generator` and `flatpak-node-generator`, rather than the
  one in `packaging/flatpak` that repackages the release's `.deb`.

## Now

Where Arcadex is behind Retrom, the nearest open-source alternative, and
the things its users keep asking for that it has not built. Arcadex is
ahead on knowing what a file is - hashes, headers, disc filesystems, DATs -
on saves and play time across machines without a server, and on everything
it does with a single machine's library.

- [ ] **Add to Steam.** Non-Steam shortcuts with grid art, for playing
      from Steam's own interface on a Deck.
- [ ] **Show the artwork we already have.** The scraper fetches title
      screens and in-game shots, and only the hero image shows one.
- [ ] **Collections and saved filters.** Hand-picked lists that cut across
      systems - "beat these", "co-op", "for the kids" - and filters kept
      around, with region and language read from the dump's own name.
- [ ] **Sync for standalone emulators' saves.** Each keeps them its own
      way - memory cards, per-game folders, a NAND - so each needs teaching.

## Next

- [ ] **BIOS and firmware check, per system.** RetroArch's core `.info`
      files list what each core wants and its checksum; say what is there,
      what is missing and what is the wrong dump. Never where to get it.
- [ ] **RetroAchievements.** Log in, show what a game has and what you
      have got, and switch RetroArch's achievements on at launch the way
      cheats are written now.
- [ ] **Per-game emulator arguments.** The override takes a whole command;
      sometimes all you want is one extra flag on the system's.
- [ ] **Read a `.cue` sheet's own track list** rather than only the files
      it names, so a multi-track disc reports its real size.

## Later

- [ ] **Edit every detail by hand**: description, genre, release date.
- [ ] **A virtualised grid**, for libraries in the tens of thousands.
      Tiles already re-render only when their own game changes.

## Done

- [x] Arcadex: the new name, with libraries and saved passwords carried
      across from Playdex [0.16.0]
- [x] Saves and play time across machines, through a folder you already
      sync [0.16.0]
- [x] On the Steam Deck: a Flatpak, full screen in a couch layout from
      Game Mode, and emulators started on the host [0.16.0]
- [x] The keyboard as a pad: arrows move, Enter presses, Escape is Back,
      F11 is full screen [0.16.0]
- [x] macOS builds signed ad hoc, with instructions for Gatekeeper [0.16.0]
- [x] The Lounge shelf's rows no longer drawn over each other [0.16.0]
- [x] The site's screenshots and link preview retaken as Arcadex, all six
      layouts [0.16.0]
- [x] PlayStation 3, 4 and 5, Xbox, Xbox 360 and Wii U, with games kept
      as folders recognised as one game each [0.16.0]
- [x] Standalone emulators found on disk, and their arguments filled in
      [0.16.0]
- [x] Passwords in the system credential store, a content security
      policy, and a media protocol that serves artwork only [0.16.0]
- [x] Checksum a disc image on demand, past the 512 MB scan limit [0.16.0]
- [x] Frontend tests, and CI running both test suites [0.16.0]
- [x] Console games played straight from a zip, unpacked whole with
      progress [0.16.0]
- [x] A Getting started card that names the systems that cannot start yet
      [0.16.0]
- [x] Nothing lost by accident: cheats, saves, hacks, removal and clean-up
      fixed after a review, and nothing left freezing the window [0.16.0]

- [x] Multi-disc games gathered into one entry, with an `.m3u` the emulator
      swaps discs from [0.8.0]
- [x] Raw `.bin` tracks read as the discs they are, with no `.cue` needed
      [0.8.0]
- [x] Per-game emulator override, which cheats and saves follow [0.8.0]
- [x] Adding a folder in Settings scans it, the way dropping one already
      did [0.7.2]
- [x] Smooth scrolling on the console skin [0.7.1]
- [x] Sony disc identification - PS1, PS2, PSP read out of ISO 9660 [0.7.0]
- [x] DAT verification against No-Intro and Redump catalogues [0.6.0]
- [x] Folder-based ROM collections, cue sheets and their tracks [0.5.x]
- [x] Drag and drop a ROM or a folder onto the window [0.4.0]
- [x] Extraction cache with a size limit, and deleting the original
      archive after unpacking [0.4.x]
- [x] Three appearances - desktop, console, and library - plus controller
      navigation and remapping [0.3.0]
- [x] Metadata fetched automatically after a scan [0.2.0]
- [x] In-app updates [0.2.0]
