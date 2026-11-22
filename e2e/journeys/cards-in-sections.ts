import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import { inside, launchGarden, readBoard } from "../board-page.ts";
import type { Journey } from "../driver.ts";
import { BOARD } from "../garden.ts";

export const cardsInSections: Journey = {
  name: "cards in their sections",
  async run(driver, artifacts) {
    const { app, garden } = await launchGarden(driver);
    const { sections, notes } = garden;
    const board = await readBoard(app);

    assert.deepEqual(
      board.frames.map((frame) => [frame.id, frame.title]),
      [
        [sections.beds, "Beds"],
        [sections.raised, "Raised beds"],
        [sections.harvest, "Harvest"],
      ],
      "the window does not show every section frame with its title",
    );

    const expected: Record<string, readonly string[]> = {
      [sections.beds]: [notes.plants, notes.tomato].sort(),
      [sections.raised]: [notes.tomato],
      [sections.harvest]: [notes.peas],
    };
    for (const frame of board.frames) {
      const held = board.cards
        .filter((card) => inside(frame.box, card.box))
        .map((card) => card.id)
        .sort();
      assert.deepEqual(held, expected[frame.id], `the frame ${frame.title} holds the wrong cards`);
    }
    const swap = board.cards.find((card) => card.id === notes.swap);
    assert.ok(swap !== undefined, "the note without a section has no card");
    assert.ok(
      board.frames.every((frame) => !inside(frame.box, swap.box)),
      "the note without a section is drawn inside a frame",
    );

    const byId = new Map(board.cards.map((card) => [card.id, card]));
    const tomato = byId.get(notes.tomato);
    assert.equal(tomato?.title, "Tomato plan for the south fence");
    assert.deepEqual(tomato?.tags, ["vegetable", "chore"]);
    assert.equal(
      tomato?.html,
      "<p>Six plants along the south fence. Stake them in June.</p>\n<p>Water at the root, never the leaves.</p>",
      "the body is not rendered as Markdown paragraphs",
    );
    assert.match(byId.get(notes.swap)?.html ?? "", /<strong>beans<\/strong>/);
    assert.doesNotMatch(
      byId.get(notes.swap)?.html ?? "",
      /<img/,
      "raw HTML in a note's body is drawn as HTML",
    );
    assert.equal(
      await app.evaluate(() => "__ran" in window),
      false,
      "a script in a note's body ran in the window",
    );
    const peas = byId.get(notes.peas)?.html ?? "";
    assert.match(
      peas,
      /<li class="task-list-item"><input(?=[^>]*type="checkbox")(?=[^>]*checked="")[^>]*>\s*Basket<\/li>/,
      `a task list is not rendered as GitHub-flavoured Markdown: ${peas}`,
    );

    await writeFile(
      join(garden.workspace, BOARD, "loose-page.md"),
      "# Loose page\n\nNo id here.\n",
    );
    await app.waitFor(
      () =>
        document.querySelector("[data-strays]")?.textContent?.includes("loose-page.md") ?? false,
      { timeout: 3_000 },
    );
    assert.equal((await readBoard(app)).cards.length, 4, "a file without an id is drawn as a card");

    await app.snapshot(join(artifacts, "board-shown.png"));
    await app.quit();
  },
};
