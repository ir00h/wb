import assert from "node:assert/strict";
import { mkdir, readdir, readFile, rename } from "node:fs/promises";
import { join } from "node:path";
import { centreOf, editorText, eventually, launchGarden, openEditor } from "../board-page.ts";
import type { Journey, RunningApp } from "../driver.ts";
import { BOARD } from "../garden.ts";

const TYPED = " Kept after the file went.";
const WAIT_MS = 3_000;

async function trash(board: string, note: string): Promise<void> {
  const name = (await readdir(board)).find((entry) => entry.endsWith(`-${note}.md`));
  if (name === undefined) {
    throw new Error(`no file holds the note ${note}`);
  }
  await mkdir(join(board, ".trash"), { recursive: true });
  await rename(join(board, name), join(board, ".trash", name));
}

async function notesHolding(board: string, text: string): Promise<readonly string[]> {
  const found: string[] = [];
  for (const name of await readdir(board)) {
    if (name.endsWith(".md") && (await readFile(join(board, name), "utf8")).includes(text)) {
      found.push(name);
    }
  }
  return found;
}

function removedShown(app: RunningApp, note: string): Promise<boolean> {
  return app.waitFor(
    (id) => document.querySelector(`[data-removed="${id}"]`) !== null,
    { timeout: WAIT_MS },
    note,
  );
}

export const removedNote: Journey = {
  name: "a removed note keeps the typed text",
  async run(driver, artifacts) {
    const { app, garden } = await launchGarden(driver);
    const board = join(garden.workspace, BOARD);
    const { plants, peas } = garden.notes;

    await openEditor(app, plants);
    await app.typeText(TYPED);
    await trash(board, plants);
    await removedShown(app, plants);
    assert.equal(
      await editorText(app, plants),
      `Tomatoes, basil and beans.${TYPED}`,
      "the typed text left the editor when its note was removed",
    );
    await app.press("Escape");
    assert.equal(
      await editorText(app, plants),
      `Tomatoes, basil and beans.${TYPED}`,
      "Escape dropped the typed text of a removed note",
    );
    await app.snapshot(join(artifacts, "removed-note.png"));
    await app.click(await centreOf(app, "[data-restore]"));
    assert.ok(
      await eventually(async () => (await notesHolding(board, TYPED)).length === 1, WAIT_MS),
      "saving the typed text as a new note did not write one note file",
    );
    const [restored] = await notesHolding(board, TYPED);
    const text = await readFile(join(board, restored ?? ""), "utf8");
    assert.match(text, /^section: Beds$/m, `the new note is not in the same section:\n${text}`);
    assert.match(text, /^# Plant list$/m, `the new note lost its title:\n${text}`);
    await app.waitFor(() => document.querySelector("[data-removed]") === null, {
      timeout: WAIT_MS,
    });

    await openEditor(app, peas);
    await app.typeText(" Dropped on purpose.");
    await trash(board, peas);
    await removedShown(app, peas);
    await app.click(await centreOf(app, "[data-discard]"));
    await app.waitFor(
      (id) => document.querySelector(`[data-card="${id}"]`) === null,
      { timeout: WAIT_MS },
      peas,
    );
    assert.deepEqual(
      await notesHolding(board, "Dropped on purpose."),
      [],
      "discarding wrote the typed text somewhere",
    );
    await app.quit();
  },
};
