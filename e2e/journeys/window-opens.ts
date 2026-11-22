import assert from "node:assert/strict";
import { join } from "node:path";
import type { Journey } from "../driver.ts";
import { readPng } from "../png.ts";

const PRODUCT_NAME = "Whiteboard";
const FRAME_WAIT_MS = 1_000;

export const windowOpens: Journey = {
  name: "window opens",
  async run(driver, artifacts) {
    const app = await driver.launch();
    await app.waitFor(
      (name) => document.querySelector("h1")?.textContent === name,
      { timeout: 5_000 },
      PRODUCT_NAME,
    );

    const probe = await app.evaluate(
      (label, times) => `${label} ${times * document.title.length} ${document.title}`,
      "probe",
      3,
    );
    assert.equal(
      probe,
      `probe ${3 * PRODUCT_NAME.length} ${PRODUCT_NAME}`,
      "the channel did not run the function with its arguments in the page",
    );

    const facts = await app.window();
    assert.equal(facts.title, PRODUCT_NAME);
    assert.equal(facts.visible, true, "the window is not visible on screen");

    const rendering = await app.evaluate(
      (wait) =>
        new Promise<{ visibility: DocumentVisibilityState; framed: boolean }>((resolve) => {
          const settle = (framed: boolean) =>
            resolve({ visibility: document.visibilityState, framed });
          requestAnimationFrame(() => settle(true));
          setTimeout(() => settle(false), wait);
        }),
      FRAME_WAIT_MS,
    );
    assert.deepEqual(
      rendering,
      { visibility: "visible", framed: true },
      "the page is not rendering: it is hidden or draws no animation frame",
    );

    const lines = await app.evaluate(() =>
      Array.from(document.querySelectorAll("main > *"), (node) => node.textContent ?? ""),
    );
    assert.deepEqual(lines, [PRODUCT_NAME, "No board open", "Desktop shell"]);

    const heading = await app.evaluate(() => {
      const heading = document.querySelector("h1");
      if (heading === null) {
        return "there is no heading";
      }
      const box = heading.getBoundingClientRect();
      const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
      if (hit !== heading) {
        return `the element at the heading's centre is ${hit === null ? "nothing" : hit.outerHTML.slice(0, 120)}`;
      }
      const style = getComputedStyle(heading);
      if (Number(style.opacity) === 0 || style.color.replace(/\s/g, "").endsWith(",0)")) {
        return `the heading is drawn invisible (opacity ${style.opacity}, colour ${style.color})`;
      }
      return "seen";
    });
    assert.equal(heading, "seen", `the heading cannot be seen: ${heading}`);

    const shot = join(artifacts, "window-opens.png");
    await app.snapshot(shot);
    const png = await readPng(shot);
    assert.deepEqual(
      { width: png.width, height: png.height },
      {
        width: Math.round(facts.width * facts.scale),
        height: Math.round(facts.height * facts.scale),
      },
    );
    assert.ok(png.colours > 1, `the snapshot is blank: it holds ${png.colours} colour`);

    await app.quit();
    assert.equal(app.isRunning(), false);
  },
};
