import { create } from "zustand";
import type { BoardId } from "../bindings/BoardId";
import { runsInDesktopShell } from "../bridge/platform";

export type AppState = {
  readonly board: BoardId | null;
  readonly inDesktopShell: boolean;
};

export const useApp = create<AppState>(() => ({
  board: null,
  inDesktopShell: runsInDesktopShell(),
}));
