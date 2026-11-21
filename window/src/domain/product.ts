import type { BoardId } from "../bindings/BoardId";

export const PRODUCT_NAME = "Whiteboard";

export function windowTitle(board: BoardId | null): string {
  return board === null ? PRODUCT_NAME : `${board} · ${PRODUCT_NAME}`;
}
