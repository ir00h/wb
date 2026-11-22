import type { BoardEvent } from "./BoardEvent";
import type { BoardId } from "./BoardId";

export type BoardUpdate = { board: BoardId, events: Array<BoardEvent>, };
