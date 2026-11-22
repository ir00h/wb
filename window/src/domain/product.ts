export const PRODUCT_NAME = "Whiteboard";

export function windowTitle(board: string | null): string {
  return board === null ? PRODUCT_NAME : `${board} · ${PRODUCT_NAME}`;
}
