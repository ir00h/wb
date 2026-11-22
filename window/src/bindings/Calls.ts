import type { ApplyOp } from "./ApplyOp";
import type { BoardSnapshot } from "./BoardSnapshot";
import type { Boards } from "./Boards";
import type { BoardUpdate } from "./BoardUpdate";
import type { OpenBoard } from "./OpenBoard";
import type { Outcome } from "./Outcome";

export type Calls = {
  listBoards: { args: null, reply: Boards };
  openBoard: { args: OpenBoard, reply: BoardSnapshot };
  apply: { args: ApplyOp, reply: Outcome };
};

export const BOARD_EVENT = "board";
export type BoardEventPayload = BoardUpdate;

export const ERROR_TYPES = ["workspaceNotFound", "boardNotFound", "noteNotFound", "duplicateNote", "notAPlainFile", "sectionNotFound", "invalid", "busy", "locksUnsupported", "malformed", "io", "badCall", "noBoardToOpen"] as const;

export const TITLE_BAR = { height: 48, buttonsX: 18, buttonsY: 26, contentInset: 94 } as const;
