//! Standalone emulators Playdex knows the command line of.
//!
//! Browsing to an emulator in Settings fills in a command for you. For most
//! that is `"emulator" "{rom}"`, but several want something more, and the
//! ones that need it most are the ones people are least likely to guess:
//! RPCS3 opens its game list unless told `--no-gui`, and PCSX2 and
//! DuckStation stay open after the game closes unless told `-batch`, which
//! also keeps play time counting long after you stopped playing.
//!
//! Recognising an emulator is by the name of its executable, which is all
//! there is to go on before anything has run. Nothing is downloaded, and
//! nothing here says where to get an emulator, its firmware or its keys.

use serde::Serialize;

struct Known {
    /// What the emulator is called, for showing.
    name: &'static str,
    /// Lowercase starts of the executable's file name. Builds get named
    /// `pcsx2-qt`, `duckstation-qt-x64-ReleaseLTCG`, `PPSSPPWindows64`.
    stems: &'static [&'static str],
    /// Arguments, with `{rom}` where the game goes.
    args: &'static str,
}

/// Longer, more specific stems first, since the first match wins.
const KNOWN: &[Known] = &[
    // --- PlayStation
    Known { name: "RPCS3", stems: &["rpcs3"], args: r#"--no-gui "{rom}""# },
    // The emulator itself, not the separate Qt launcher, which takes
    // different arguments.
    Known { name: "shadPS4", stems: &["shadps4"], args: r#"-g "{rom}""# },
    Known { name: "PCSX2", stems: &["pcsx2"], args: r#"-batch "{rom}""# },
    Known { name: "DuckStation", stems: &["duckstation"], args: r#"-batch "{rom}""# },
    Known { name: "PPSSPP", stems: &["ppsspp"], args: r#""{rom}""# },
    // --- Microsoft
    Known { name: "Xenia", stems: &["xenia"], args: r#""{rom}""# },
    Known { name: "xemu", stems: &["xemu"], args: r#"-dvd_path "{rom}""# },
    // --- Nintendo
    Known { name: "Cemu", stems: &["cemu"], args: r#"-g "{rom}""# },
    Known { name: "Dolphin", stems: &["dolphin"], args: r#"-b -e "{rom}""# },
    // The Switch emulators descended from one codebase share its arguments.
    Known {
        name: "a Switch emulator",
        stems: &["eden", "citron", "sudachi", "suyu", "torzu", "yuzu"],
        args: r#"-g "{rom}""#,
    },
    Known { name: "Ryujinx", stems: &["ryujinx"], args: r#""{rom}""# },
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    /// The whole command, executable quoted, ready to save.
    pub command: String,
    /// The emulator it was recognised as, when it was.
    pub emulator: Option<String>,
}

/// The command to run a game with the emulator at `exe`.
pub fn suggest(exe: &str) -> Suggestion {
    let exe = crate::detect::clean_path(exe);
    let stem = std::path::Path::new(&exe)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let known = KNOWN
        .iter()
        .find(|k| k.stems.iter().any(|s| stem.starts_with(s)));
    match known {
        Some(k) => Suggestion {
            command: format!(r#""{exe}" {}"#, k.args),
            emulator: Some(k.name.to_string()),
        },
        None => Suggestion {
            command: format!(r#""{exe}" "{{rom}}""#),
            emulator: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_in_what_each_emulator_needs() {
        assert_eq!(
            suggest(r"C:\Emulators\RPCS3\rpcs3.exe").command,
            r#""C:\Emulators\RPCS3\rpcs3.exe" --no-gui "{rom}""#
        );
        assert_eq!(
            suggest(r"D:\xenia\xenia_canary.exe").emulator.as_deref(),
            Some("Xenia")
        );
        assert_eq!(
            suggest(r"C:\PCSX2\pcsx2-qt.exe").command,
            r#""C:\PCSX2\pcsx2-qt.exe" -batch "{rom}""#
        );
        assert_eq!(suggest("/usr/bin/eden").command, r#""/usr/bin/eden" -g "{rom}""#);
        assert_eq!(suggest(r"C:\Cemu\Cemu.exe").emulator.as_deref(), Some("Cemu"));
    }

    /// Explorer's "Copy as path" quotes are stripped, or the command would
    /// end up with two sets.
    #[test]
    fn strips_pasted_quotes() {
        assert_eq!(
            suggest(r#""C:\xemu\xemu.exe""#).command,
            r#""C:\xemu\xemu.exe" -dvd_path "{rom}""#
        );
    }

    #[test]
    fn anything_else_gets_the_plain_command() {
        let s = suggest(r"C:\Emulators\Mystery\mystery.exe");
        assert_eq!(s.command, r#""C:\Emulators\Mystery\mystery.exe" "{rom}""#);
        assert_eq!(s.emulator, None);
    }
}
