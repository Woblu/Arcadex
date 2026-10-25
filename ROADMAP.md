# Roadmap

Things worth building, roughly in the order they'd help. Checked items
have shipped; the version they shipped in is in brackets, or "next" for
what is done but not yet released.

## Now

- [ ] **Playlists and collections.** Hand-picked lists that cut across
      systems - "beat these", "co-op", "for the kids".
- [ ] **Show the artwork we already have.** The scraper fetches
      title-screen and in-game shots and nothing displays them.
- [ ] **Sign the Windows installers.** The fix for Defender deleting them.
      SignPath Foundation signs open source projects for free, but it
      takes an application from the maintainer; after that it is a step in
      `release.yml`. See the README.

## Next

- [ ] **Per-game emulator arguments.** The override takes a whole command;
      sometimes all you want is one extra flag on the system's.
- [ ] **Read a `.cue` sheet's own track list** rather than only the files
      it names, so a multi-track disc reports its real size.

## Later

- [ ] **RetroAchievements.** Log in, show what a game has, show what you
      have got.
- [ ] **Saved filters.** "Unplayed SNES games under 2 MB", kept around.
- [ ] **Run it against a real library.** Thousands of files, not the
      handful used while building this.

## Done

- [x] PlayStation 3, 4 and 5, Xbox, Xbox 360 and Wii U, with games kept
      as folders recognised as one game each [next]
- [x] Standalone emulators found on disk, and their arguments filled in
      [next]
- [x] Passwords in the system credential store, a content security
      policy, and a media protocol that serves artwork only [next]
- [x] Checksum a disc image on demand, past the 512 MB scan limit [next]
- [x] Frontend tests, and CI running both test suites [next]

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
