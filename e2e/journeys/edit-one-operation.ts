import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import { editorText, launchGarden, openEditor } from "../board-page.ts";
import type { AppliedCount, Journey, RunningApp } from "../driver.ts";
import { BOARD, changedFiles, TOMATO_BODY, treeHashes } from "../garden.ts";

const POLL_MS = 50;
const SAVE_WAIT_MS = 5_000;

async function noteFile(board: string, note: string): Promise<string> {
  const name = (await readdir(board)).find((entry) => entry.endsWith(`-${note}.md`));
  if (name === undefined) {
    throw new Error(`no file holds the note ${note}`);
  }
  return name;
}

async function appliedTo(app: RunningApp, note: string): Promise<readonly AppliedCount[]> {
  return (await app.applied()).filter((entry) => entry.note === note || entry.note === null);
}

async function waitForEdits(app: RunningApp, note: string, count: number): Promise<void> {
  const deadline = Date.now() + SAVE_WAIT_MS;
  let last: readonly AppliedCount[] = [];
  while (Date.now() < deadline) {
    last = await app.applied();
    if (
      last.some((entry) => entry.kind === "editNote" && entry.note === note && entry.count >= count)
    ) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, POLL_MS));
  }
  throw new Error(`the shell did not apply ${count} edit(s) of ${note}: ${JSON.stringify(last)}`);
}

export const editOneOperation: Journey = {
  name: "an edit is one operation on that note",
  async run(driver, artifacts) {
    const { app, garden } = await launchGarden(driver);
    const tomato = garden.notes.tomato;
    const board = join(garden.workspace, BOARD);
    const before = await treeHashes(board);

    await openEditor(app, tomato);
    await app.snapshot(join(artifacts, "editor-open.png"));
    await app.typeText(" Mulch in July.");
    assert.equal(await editorText(app, tomato), `${TOMATO_BODY} Mulch in July.`);
    await waitForEdits(app, tomato, 1);
    assert.deepEqual(
      await appliedTo(app, tomato),
      [{ kind: "editNote", note: tomato, count: 1 }],
      "a pause in typing did not save exactly one edit of the note",
    );

    await app.typeText(" Weekly.");
    await app.press("Escape");
    await app.waitFor(
      (id) => document.querySelector(`[data-editor="${id}"]`) === null,
      { timeout: 3_000 },
      tomato,
    );
    await waitForEdits(app, tomato, 2);
    assert.deepEqual(
      await appliedTo(app, tomato),
      [{ kind: "editNote", note: tomato, count: 2 }],
      "closing the editor did not save exactly one more edit of the note",
    );

    const file = await noteFile(board, tomato);
    const text = await readFile(join(board, file), "utf8");
    assert.ok(
      text.includes("Water at the root, never the leaves. Mulch in July. Weekly.\n"),
      `the note's file does not hold the typed text:\n${text}`,
    );
    await app.waitFor(
      (id) =>
        document
          .querySelector(`[data-card="${id}"] .card-body`)
          ?.textContent?.endsWith("Weekly.") ?? false,
      { timeout: 3_000 },
      tomato,
    );
    assert.deepEqual(
      changedFiles(before, await treeHashes(board)),
      [file],
      "the save changed files besides the note's own",
    );
    await app.quit();
  },
};
