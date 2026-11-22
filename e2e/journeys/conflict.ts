import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import { centreOf, editorText, eventually, launchGarden, openEditor } from "../board-page.ts";
import type { Driver, Journey } from "../driver.ts";
import { BOARD, wb } from "../garden.ts";

const ORIGINAL = "Tomatoes, basil and beans.";
const TYPED = " And chard.";
const THEIRS = "Only potatoes this year.";

type Choice = "take theirs" | "keep mine";

async function bodyOnDisk(workspace: string, note: string): Promise<string> {
  const board = join(workspace, BOARD);
  const name = (await readdir(board)).find((entry) => entry.endsWith(`-${note}.md`));
  if (name === undefined) {
    throw new Error(`no file holds the note ${note}`);
  }
  const text = await readFile(join(board, name), "utf8");
  return text.slice(text.indexOf("\n\n", text.indexOf("# ")) + 2).trimEnd();
}

async function clash(driver: Driver, choice: Choice, artifacts: string): Promise<void> {
  const { app, garden } = await launchGarden(driver);
  const note = garden.notes.plants;
  await openEditor(app, note);
  await app.typeText(TYPED);
  await wb(garden.workspace, "note", "edit", note, "--body", THEIRS);
  await app.press("Escape");

  await app.waitFor(
    (id) => document.querySelector(`[data-conflict="${id}"]`) !== null,
    { timeout: 3_000 },
    note,
  );
  assert.equal(await bodyOnDisk(garden.workspace, note), THEIRS, "the outside text is not on disk");
  assert.deepEqual(
    await app.operations(),
    [{ kind: "editNote", note, outcome: "conflict", count: 1 }],
    "closing the editor did not send one save, based on the text it started from, before the outside change arrived",
  );
  assert.equal(await editorText(app, note), ORIGINAL + TYPED, "a typed keystroke was lost");
  assert.equal(
    await app.evaluate(() => document.querySelector("[data-theirs]")?.textContent ?? null),
    THEIRS,
    "the editor does not show the text on disk beside the typed text",
  );
  await app.press("Escape");
  assert.equal(
    await editorText(app, note),
    ORIGINAL + TYPED,
    "Escape closed the editor while the typed text was unsaved",
  );
  if (choice === "take theirs") {
    await app.snapshot(join(artifacts, "conflict-shown.png"));
  }

  const button = choice === "take theirs" ? "[data-take-theirs]" : "[data-keep-mine]";
  await app.click(await centreOf(app, button));
  if (choice === "take theirs") {
    await app.waitFor(
      ([id, theirs]) =>
        document.querySelector<HTMLTextAreaElement>(`[data-editor="${id}"]`)?.value === theirs,
      { timeout: 3_000 },
      [note, THEIRS] as const,
    );
    assert.equal(await bodyOnDisk(garden.workspace, note), THEIRS);
  } else {
    await eventually(
      async () => (await bodyOnDisk(garden.workspace, note)) === ORIGINAL + TYPED,
      3_000,
    );
    assert.equal(
      await bodyOnDisk(garden.workspace, note),
      ORIGINAL + TYPED,
      "keep mine did not write the typed text",
    );
  }
  await app.quit();
}

export const conflictTakeTheirs: Journey = {
  name: "a conflict keeps both texts, take theirs",
  run: (driver, artifacts) => clash(driver, "take theirs", artifacts),
};

export const conflictKeepMine: Journey = {
  name: "a conflict keeps both texts, keep mine",
  run: (driver, artifacts) => clash(driver, "keep mine", artifacts),
};
