# Contributing

Thanks for wanting to help. Bug reports, ideas and pull requests are all
welcome.

## Building

```bash
npm install
npm run tauri dev      # run it
npm test               # frontend tests
cd src-tauri && cargo test
```

The README's **Setup** section lists what each platform needs. CI runs both
test suites on every push, the Rust ones on Linux and Windows, so a pull
request shows within a few minutes whether it builds everywhere.

## How the code is written

- **Comments say why.** The code says what. Most functions here carry a
  paragraph about the case that made them necessary - a real dump, a real
  emulator, a real mistake - and that is the part worth keeping when the
  code around it changes.
- **Tests are named for the behaviour, in a sentence.**
  `a_drive_that_is_not_there_is_not_a_deleted_library`, not
  `test_missing_2`. A failing test should say what broke.
- **Nothing is deleted on a guess.** Arcadex only ever removes a file of
  yours when asked directly, and says so first. A heuristic that drops a
  library entry leaves anything played, favourited or patched alone.
- **Slow work stays off the main thread.** A Tauri command that is not
  `async` freezes the window until it returns. Anything that reads a
  folder, an archive or the network goes through `off_main` or is `async`.
- **Plain words in the interface.** "Unpack and keep only the game", not
  "Extract archive (destructive)".

## What Arcadex will not do

Pull requests that search for, download or link to games, BIOS files,
firmware, decryption keys or dumping tools will be closed, as will ones that
put a console maker's name, logo or lettering into the app's own interface.
Detecting an emulator someone installed, and telling them what it needs, is
fine; fetching it for them is not.
