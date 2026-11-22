import assert from "node:assert/strict";
import { cp, link, readdir, readFile, rename, rm, utimes, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { cardsShown, eventually, launchGarden } from "../board-page.ts";
import type { Journey, RunningApp } from "../driver.ts";
import { addNote, BOARD } from "../garden.ts";

const RECHECK_WAIT_MS = 5_000;
const EVENT_WAIT_MS = 1_500;
const BUSY_WRITE_MS = 60;
const FIXED_TIME = new Date("2026-04-01T08:00:00Z");
const UNTICKED = "Round one: [ ] water the beds.";
const TICKED = "Round one: [x] water the beds.";

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function noteFile(id: string, title: string): string {
  return `---\nid: ${id}\nsection: Beds\ntags: [plan]\n---\n\n# ${title}\n\nWritten while the window watched.\n`;
}

function cardBody(app: RunningApp, note: string): Promise<string | null> {
  return app.evaluate(
    (id) => document.querySelector(`[data-card="${id}"] .card-body`)?.textContent ?? null,
    note,
  );
}

function problemShown(app: RunningApp): Promise<string | null> {
  return app.evaluate(() =>
    document.querySelector(".canvas.stale") === null
      ? null
      : (document.querySelector("[data-board-problem]")?.textContent ?? null),
  );
}

async function cardAppears(app: RunningApp, note: string, timeout: number): Promise<boolean> {
  return eventually(
    () => app.evaluate((id) => document.querySelector(`[data-card="${id}"]`) !== null, note),
    timeout,
  );
}

export const boardFolder: Journey = {
  name: "the window follows its board folder",
  async run(driver, artifacts) {
    const { app, garden } = await launchGarden(driver);
    const workspace = garden.workspace;
    const board = join(workspace, BOARD);

    const plantFile = (await readdir(board)).find((name) =>
      name.endsWith(`-${garden.notes.plants}.md`),
    );
    assert.ok(plantFile !== undefined, "the plant list has no file");
    const linked = join(dirname(workspace), "plant-list-link.md");
    await link(join(board, plantFile), linked);
    const viaLink = (await readFile(linked, "utf8")).replace(
      "Tomatoes, basil and beans.",
      "Changed through another name of the file.",
    );
    await writeFile(linked, viaLink);
    assert.ok(
      await eventually(
        async () =>
          (await cardBody(app, garden.notes.plants)) ===
          "Changed through another name of the file.",
        RECHECK_WAIT_MS,
      ),
      "a change the watcher did not report was not caught by the recheck",
    );

    const plantPath = join(board, plantFile);
    const saveKeepingTime = async (from: string, to: string) => {
      await writeFile(plantPath, (await readFile(plantPath, "utf8")).replace(from, to));
      await utimes(plantPath, FIXED_TIME, FIXED_TIME);
    };
    await saveKeepingTime("Changed through another name of the file.", UNTICKED);
    assert.ok(
      await eventually(
        async () => (await cardBody(app, garden.notes.plants)) === UNTICKED,
        EVENT_WAIT_MS,
      ),
      "a save with the modification time set back was not shown",
    );
    await saveKeepingTime(UNTICKED, TICKED);
    assert.ok(
      await eventually(
        async () => (await cardBody(app, garden.notes.plants)) === TICKED,
        EVENT_WAIT_MS,
      ),
      "a second save of the same size and modification time was not shown",
    );

    let busy = true;
    const busyFile = join(board, "busy-log.txt");
    const writer = (async () => {
      for (let round = 0; busy; round += 1) {
        await writeFile(busyFile, `line ${round}\n`);
        await sleep(BUSY_WRITE_MS);
      }
    })();
    await sleep(300);
    const started = Date.now();
    const amid = await addNote(workspace, "Amid busy writes", "Beds", ["plan"], "Shown anyway.");
    const shownAmid = await cardAppears(app, amid, EVENT_WAIT_MS);
    const waited = Date.now() - started;
    busy = false;
    await writer;
    assert.ok(
      shownAmid,
      `a file written every ${BUSY_WRITE_MS} ms kept a new note out of the window for ${waited} ms`,
    );

    const config = join(board, "board.config.json");
    const goodConfig = await readFile(config, "utf8");
    await writeFile(config, "{ not json");
    assert.ok(
      await eventually(async () => (await problemShown(app)) !== null, EVENT_WAIT_MS),
      "an unreadable board.config.json is shown as if nothing happened",
    );
    assert.match((await problemShown(app)) ?? "", /board\.config\.json/);
    await app.snapshot(join(artifacts, "board-unreadable.png"));
    assert.equal(
      await app.evaluate(() => document.querySelectorAll("[data-card]").length),
      5,
      "the last good cards did not stay in view",
    );
    await writeFile(join(board, "while-broken-nbroken1.md"), noteFile("nbroken1", "While broken"));
    await writeFile(config, goodConfig);
    assert.ok(
      await cardAppears(app, "nbroken1", EVENT_WAIT_MS),
      "a note added while the config was broken did not appear once it was repaired",
    );
    assert.equal(await problemShown(app), null, "the notice stayed after the config was repaired");

    const away = join(workspace, "garden-away");
    await rename(board, away);
    assert.ok(
      await eventually(async () => (await problemShown(app)) !== null, EVENT_WAIT_MS),
      "a board folder that is gone is shown as if nothing happened",
    );
    await writeFile(join(away, "written-away-naway001.md"), noteFile("naway001", "Written away"));
    await rename(away, board);
    assert.ok(
      await cardAppears(app, "naway001", EVENT_WAIT_MS),
      "the board folder came back with one more note and the window did not show it",
    );
    assert.equal(await problemShown(app), null, "the notice stayed after the folder came back");

    const copy = join(workspace, "garden-copy");
    await cp(board, copy, { recursive: true });
    await writeFile(
      join(copy, "only-in-the-copy-ncopy001.md"),
      noteFile("ncopy001", "Only in the copy"),
    );
    await rename(board, join(workspace, "garden-old"));
    await rename(copy, board);
    assert.ok(
      await cardAppears(app, "ncopy001", EVENT_WAIT_MS),
      "the board folder was swapped for a copy and the window did not show it",
    );
    await rm(join(workspace, "garden-old"), { recursive: true, force: true });
    await cardsShown(app, 8);
    await app.quit();
  },
};
