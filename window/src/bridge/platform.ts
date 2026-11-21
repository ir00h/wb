import { isTauri } from "@tauri-apps/api/core";

export function runsInDesktopShell(): boolean {
  return isTauri();
}
