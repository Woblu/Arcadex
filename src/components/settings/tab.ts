import type { Dispatch, SetStateAction } from "react";

import type { Settings } from "../../types";

/**
 * What every Settings tab is handed.
 *
 * The settings themselves stay with the window, because the Save button at
 * the bottom saves fields from several tabs at once. Everything else a tab
 * shows - its folders, its catalogue, its downloads - is its own.
 */
export interface TabProps {
  settings: Settings;
  /** Change one setting. Kept until Save, like every field in the window. */
  setValue: (key: string, value: string) => void;
  setSettings: Dispatch<SetStateAction<Settings>>;
  /** Show a failure in the banner at the top, or clear it with null. */
  onError: (message: string | null) => void;
  /** Show a short note beside the Save button. */
  onMessage: (message: string | null) => void;
  /**
   * True while this tab is the one showing. A tab stays mounted once it has
   * been opened, so an import or a download running in it carries on, and is
   * still reporting, after a look at another tab.
   */
  active: boolean;
}
