# Security

## Reporting a problem

Please report a security problem privately, through **Security → Report a
vulnerability** on this repository, rather than in a public issue. You will
get a reply within a week, and a fix or a plan for one as soon as the
problem is understood. Say whether you would like to be credited.

## What counts

Arcadex runs programs - emulators - with paths and arguments it builds, reads
files from anywhere you point it, fetches artwork from the internet and
updates itself. A problem in any of that is in scope:

- Anything that lets a downloaded description, title, image or archive run
  code, read files outside what it should, or change a setting.
- An archive that can write outside the folder it is unpacked into.
- An update that installs without a valid signature.
- Saved passwords or keys reaching the library database, logs or the page.

Problems in the emulators themselves, in RetroArch, or in the metadata
services belong to those projects.

## What is already in place

- Updates are verified against a public key compiled into the app before
  any of it runs.
- Release builds carry a provenance attestation:
  `gh attestation verify <file> --repo Woblu/Arcadex`.
- The interface runs under a content security policy with no outside
  network access; every download happens in Rust.
- The private media protocol serves images only, from the artwork folder.
- Passwords and API keys are kept in the system's credential store.
- Archive entries that would land outside their folder stop the unpack.
