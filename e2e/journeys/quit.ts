import assert from "node:assert/strict";
import { readdir, readFile, rename } from "node:fs/promises";
import { join } from "node:path";
import { centreOf, editorText, launchGarden, openEditor } from "../board-page.ts";
import type { Journey, WayOut } from "../driver.ts";
import { BOARD, wb } from "../garden.ts";

const WAYS_OUT: readonly WayOut[] = ["quit", "close", "terminate"];
const DELAYS_MS: readonly number[] = [0, 300];
const EXIT_WAIT_MS = 5_000;
const STAYS_OPEN_MS = 1_500;
const THEIRS = "Only potatoes this year.";
const HOMELESS = " Typed while the board was gone.";

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

async function noteText(workspace: string, note: string): Promise<string> {
  const board = join(workspace, BOARD);
  const name = (await readdir(board)).find((entry) => entry.endsWith(`-${note}.md`));
  if (name === undefined) {
    throw new Error(`no file holds the note ${note}`);
  }
  return readFile(join(board, name), "utf8");
}

export const quitKeepsTypedText: Journey = {
  name: "typed text survives every way out",
  async run(driver) {
    for (const way of WAYS_OUT) {
      for (const delay of DELAYS_MS) {
        const { app, garden } = await launchGarden(driver);
        const note = garden.notes.plants;
        const typed = ` Typed before ${way} after ${delay} ms.`;
        await openEditor(app, note);
        await app.typeText(typed);
        await sleep(delay);
        await app.askToQuit(way);
        assert.ok(
          await app.exited(EXIT_WAIT_MS),
          `the app did not exit within ${EXIT_WAIT_MS} ms of ${way}`,
        );
        const text = await noteText(garden.workspace, note);
        assert.ok(
          text.includes(typed),
          `the text typed ${delay} ms before ${way} is not in the note's file:\n${text}`,
        );
      }
    }

    const { app, garden } = await launchGarden(driver);
    const note = garden.notes.plants;
    await openEditor(app, note);
    await app.typeText(" Kept through a conflict.");
    await wb(garden.workspace, "note", "edit", note, "--body", THEIRS);
    await app.askToQuit("terminate");
    assert.equal(
      await app.exited(STAYS_OPEN_MS),
      false,
      "the app quit while a conflict held typed text",
    );
    await app.waitFor(
      (id) =>
        document.querySelector(`[data-conflict="${id}"]`) !== null &&
        (document.querySelector(".toast")?.textContent?.includes("did not quit") ?? false),
      { timeout: 3_000 },
      note,
    );
    assert.equal(
      await editorText(app, note),
      "Tomatoes, basil and beans. Kept through a conflict.",
      "the typed text left the editor",
    );
    await app.click(await centreOf(app, "[data-keep-mine]"));
    await app.askToQuit("close");
    assert.ok(await app.exited(EXIT_WAIT_MS), "the app did not exit once the conflict was settled");
    assert.ok(
      (await noteText(garden.workspace, note)).includes(
        "Tomatoes, basil and beans. Kept through a conflict.",
      ),
      "keep mine before quitting did not write the typed text",
    );

    const gone = await launchGarden(driver);
    const homeless = gone.garden.notes.plants;
    await openEditor(gone.app, homeless);
    await gone.app.typeText(HOMELESS);
    const board = join(gone.garden.workspace, BOARD);
    const away = join(gone.garden.workspace, "garden-away");
    await rename(board, away);
    await gone.app.askToQuit("terminate");
    assert.equal(
      await gone.app.exited(STAYS_OPEN_MS),
      false,
      "the app quit while typed text could not be saved",
    );
    await gone.app.waitFor(
      () => document.querySelector(".toast [data-quit-without-saving]") !== null,
      { timeout: 6_000 },
    );
    assert.equal(
      await editorText(gone.app, homeless),
      `Tomatoes, basil and beans.${HOMELESS}`,
      "the typed text left the editor while the board was gone",
    );
    await gone.app.click(await centreOf(gone.app, "[data-quit-without-saving]"));
    assert.ok(
      await gone.app.exited(EXIT_WAIT_MS),
      "the app did not exit after Quit without saving",
    );
    await rename(away, board);
    assert.ok(
      !(await noteText(gone.garden.workspace, homeless)).includes(HOMELESS),
      "Quit without saving wrote the typed text",
    );
  },
};
