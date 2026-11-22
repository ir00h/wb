import { describe, expect, it } from "vitest";
import { noteId } from "./boards.fixture";
import { IDLE, type InputEvent, step } from "./interaction";

const plantList = noteId("n1");
const editing = step(IDLE, { type: "doubleClicked", note: plantList }).state;

describe("interaction", () => {
  it("opens the editor of the card that was double clicked", () => {
    expect(step(IDLE, { type: "doubleClicked", note: plantList })).toEqual({
      state: { kind: "editing", note: plantList },
      effects: [{ type: "editorOpened", note: plantList }],
    });
  });

  it.each<InputEvent>([
    { type: "escape" },
    { type: "blur" },
    { type: "boardReplaced" },
    { type: "noteRemoved", note: plantList },
  ])("leaves editing on $type", (event) => {
    expect(step(editing, event)).toEqual({
      state: IDLE,
      effects: [{ type: "editorClosed", note: plantList }],
    });
  });

  it("closes one editor before it opens another", () => {
    const seeds = noteId("n2");
    expect(step(editing, { type: "doubleClicked", note: seeds }).effects).toEqual([
      { type: "editorClosed", note: plantList },
      { type: "editorOpened", note: seeds },
    ]);
  });

  it("ignores the removal of a note it is not editing", () => {
    expect(step(editing, { type: "noteRemoved", note: noteId("n2") }).state).toBe(editing);
  });
});
