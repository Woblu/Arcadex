import { useEffect, useState } from "react";

import * as api from "../../api";
import type { EmulatorConfig, Game, PlatformInfo } from "../../types";
import CommitField from "../CommitField";

interface Props {
  game: Game;
  platforms: PlatformInfo[];
  /** The emulator changed, so the launch command shown above is stale. */
  onSaved: () => Promise<void>;
  onError: (message: string) => void;
}

/** One game's own emulator, overriding its system's. */
export default function EmulatorSection({ game, platforms, onSaved, onError }: Props) {
  /** This game's own emulator, when it has been given one. */
  const [emu, setEmu] = useState<EmulatorConfig | null>(null);
  const [emuOpen, setEmuOpen] = useState(false);

  useEffect(() => {
    api.gameEmulator(game.id).then(setEmu).catch(() => setEmu(null));
  }, [game.id]);

  /// Saving re-previews the command, so the line above is what will run
  /// rather than what would have run a moment ago.
  const saveEmu = async (next: EmulatorConfig | null) => {
    setEmu(next);
    try {
      await api.saveGameEmulator(game.id, next);
      await onSaved();
    } catch (e) {
      onError(api.errorMessage(e));
    }
  };

  /// The command for an emulator picked from disk, with whatever it is known
  /// to need beyond the path.
  const browse = async (): Promise<string | null> => {
    const exe = await api.pickFile();
    if (!exe) return null;
    return (await api.suggestEmulatorCommand(exe)).command;
  };

  const systemName = api.platformName(platforms, game.platform);

  return (
    <>
      <div className="section-title">Emulator</div>
      {emu ? (
        <>
          <div className="row">
            <select
              className="select-inline"
              value={emu.mode}
              onChange={(e) =>
                void saveEmu({ ...emu, mode: e.target.value as "retroarch" | "custom" })
              }
            >
              <option value="retroarch">RetroArch</option>
              <option value="custom">Standalone</option>
            </select>

            {emu.mode === "retroarch" ? (
              <CommitField
                value={emu.core ?? ""}
                placeholder="core name"
                onCommit={(core) => void saveEmu({ ...emu, core })}
              />
            ) : (
              <>
                <CommitField
                  value={emu.customCommand ?? ""}
                  placeholder={'"C:\\Emu\\emulator.exe" "{rom}"'}
                  onCommit={(customCommand) => void saveEmu({ ...emu, customCommand })}
                />
                <button
                  className="btn small"
                  onClick={async () => {
                    const command = await browse();
                    if (command) void saveEmu({ ...emu, customCommand: command });
                  }}
                >
                  Browse
                </button>
              </>
            )}
          </div>
          <div className="row" style={{ marginTop: 6 }}>
            <button className="btn small" onClick={() => void saveEmu(null)}>
              Use the {systemName} default
            </button>
          </div>
          <div className="hint" style={{ marginTop: 6 }}>
            This game only. Cheats and saves follow it, since RetroArch files
            both under whichever core runs the game.
          </div>
        </>
      ) : emuOpen ? (
        <>
          <div className="hint">Pick what should run this one game.</div>
          <div className="row" style={{ marginTop: 6 }}>
            <button
              className="btn small"
              onClick={() =>
                void saveEmu({
                  platform: game.platform,
                  mode: "retroarch",
                  core: "",
                  customCommand: null,
                })
              }
            >
              RetroArch core
            </button>
            <button
              className="btn small"
              onClick={async () => {
                const command = await browse();
                if (!command) return;
                void saveEmu({
                  platform: game.platform,
                  mode: "custom",
                  core: null,
                  customCommand: command,
                });
              }}
            >
              Browse for an emulator
            </button>
            <button className="btn small" onClick={() => setEmuOpen(false)}>
              Cancel
            </button>
          </div>
        </>
      ) : (
        <>
          <div className="hint">Running on whatever {systemName} is set to use.</div>
          <div className="row" style={{ marginTop: 6 }}>
            <button className="btn small" onClick={() => setEmuOpen(true)}>
              Use a different emulator for this game
            </button>
          </div>
        </>
      )}
    </>
  );
}
