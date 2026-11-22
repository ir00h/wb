import assert from "node:assert/strict";
import { mkdir, readdir, rename } from "node:fs/promises";
import { join } from "node:path";
import { eventually, inside, launchGarden, readBoard } from "../board-page.ts";
import type { Journey } from "../driver.ts";
import { addNote, BOARD, wb } from "../garden.ts";

const SHOWN_WITHIN_MS = 3_000;

export const outsideChanges: Journey = {
  name: "a change from outside appears",
  async run(driver, artifacts) {
    const { app, garden } = await launchGarden(driver);
    const { workspace, notes, sections } = garden;

    await wb(workspace, "note", "edit", notes.plants, "--body", "Beans only, this year.");
    await app.waitFor(
      (id) =>
        document.querySelector(`[data-card="${id}"] .card-body`)?.textContent ===
        "Beans only, this year.",
      { timeout: SHOWN_WITHIN_MS },
      notes.plants,
    );

    const water = await addNote(workspace, "Water the beds", "Harvest", [], "At dawn.");
    await app.waitFor(
      (id) => document.querySelector(`[data-card="${id}"]`) !== null,
      { timeout: SHOWN_WITHIN_MS },
      water,
    );
    const drawnInHarvest = await eventually(async () => {
      const shown = await readBoard(app);
      const harvest = shown.frames.find((frame) => frame.id === sections.harvest);
      const card = shown.cards.find((entry) => entry.id === water);
      return harvest !== undefined && card !== undefined && inside(harvest.box, card.box);
    }, SHOWN_WITHIN_MS);
    assert.ok(drawnInHarvest, "the new note is not drawn in its section's frame");

    const board = join(workspace, BOARD);
    const swapFile = (await readdir(board)).find((name) => name.endsWith(`-${notes.swap}.md`));
    assert.ok(swapFile !== undefined);
    await mkdir(join(board, ".trash"), { recursive: true });
    await rename(join(board, swapFile), join(board, ".trash", swapFile));
    await app.waitFor(
      (id) => document.querySelector(`[data-card="${id}"]`) === null,
      { timeout: SHOWN_WITHIN_MS },
      notes.swap,
    );

    await wb(workspace, "section", "add", "Compost", "--hue", "30");
    await app.waitFor(
      () =>
        Array.from(
          document.querySelectorAll(".section-title"),
          (title) => title.textContent,
        ).includes("Compost"),
      { timeout: SHOWN_WITHIN_MS },
    );

    await app.snapshot(join(artifacts, "outside-changes.png"));
    await app.quit();
  },
};
