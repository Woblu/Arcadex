# Roadmap

Things worth building, roughly in the order they'd help. Checked items
have shipped; the version they shipped in is in brackets, or "next" for
what is done but not yet released.

## Before the next release

What stands between the work already done and putting it in front of
people. None of it is code, which is why it is listed first.

- [ ] **A new name.** Another "PlayDex" game-library app launched in
      February 2026. The name sets the app identifier, the data folder,
      the site and every package id, so it comes before anything is
      published. A rename keeps libraries: the move from Romcade back to
      Playdex already carries one across.
- [ ] **Sign the Windows installers.** The fix for Defender deleting them,
      and winget will not accept a package Defender flags. SignPath
      Foundation signs open source projects for free, but it takes an
      application from the maintainer; after that it is a step in
      `release.yml`. See the README.
- [ ] **Notarise the macOS build.** Gatekeeper, and Homebrew's own cask
      list, both want it. It needs an Apple Developer account.
- [ ] **Look twice at the Console and Channels layouts.** They are drawn
      after well-known home screens. Nintendo's 2026 takedowns went after
      look-alike interfaces as well as emulators.
- [ ] **Run it against a real library.** Thousands of files, not the
      handful used while building this.

## Now

Where Playdex is behind Retrom, the nearest open-source alternative, and
the things its users keep asking for that it has not built. Playdex is
ahead on knowing what a file is - hashes, headers, disc filesystems, DATs -
and on everything it does with a single machine's library.

- [ ] **Saves and play time across machines, without a server.** Copy a
      game's saves into a folder the user already syncs - Syncthing,
      OneDrive, a NAS - after it closes, and back before it starts, keeping
      dated versions. Play time as a journal of sessions per machine in the
      same folder, merged on start, rather than syncing the database.
- [ ] **On the Steam Deck.** A Flathub build, opening straight into a
      couch layout in Game Mode, and launching Flatpak emulators through
      `flatpak-spawn --host`.
- [ ] **Show the artwork we already have.** The scraper fetches title
      screens and in-game shots, and only the hero image shows one.
- [ ] **Collections and saved filters.** Hand-picked lists that cut across
      systems - "beat these", "co-op", "for the kids" - and filters kept
      around, with region and language read from the dump's own name.

## Next

- [ ] **BIOS and firmware check, per system.** RetroArch's core `.info`
      files list what each core wants and its checksum; say what is there,
      what is missing and what is the wrong dump. Never where to get it.
- [ ] **RetroAchievements.** Log in, show what a game has and what you
      have got, and switch RetroArch's achievements on at launch the way
      cheats are written now.
- [ ] **Add to Steam.** Non-Steam shortcuts with grid art, for playing
      from Steam's own interface on a Deck.
- [ ] **Per-game emulator arguments.** The override takes a whole command;
      sometimes all you want is one extra flag on the system's.
- [ ] **Read a `.cue` sheet's own track list** rather than only the files
      it names, so a multi-track disc reports its real size.

## Later

- [ ] **Edit every detail by hand**: description, genre, release date.
- [ ] **A virtualised grid**, for libraries in the tens of thousands.
      Tiles already re-render only when their own game changes.

## Done

- [x] PlayStation 3, 4 and 5, Xbox, Xbox 360 and Wii U, with games kept
      as folders recognised as one game each [next]
- [x] Standalone emulators found on disk, and their arguments filled in
      [next]
- [x] Passwords in the system credential store, a content security
      policy, and a media protocol that serves artwork only [next]
- [x] Checksum a disc image on demand, past the 512 MB scan limit [next]
- [x] Frontend tests, and CI running both test suites [next]
- [x] Console games played straight from a zip, unpacked whole with
      progress [next]
- [x] A Getting started card that names the systems that cannot start yet
      [next]
- [x] Nothing lost by accident: cheats, saves, hacks, removal and clean-up
      fixed after a review, and nothing left freezing the window [next]

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
