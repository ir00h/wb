import assert from "node:assert/strict";
import { join } from "node:path";
import type { Journey } from "../driver.ts";
import { pngSize } from "../png.ts";

const PRODUCT_NAME = "Whiteboard";

export const windowOpens: Journey = {
  name: "window opens",
  async run(driver, artifacts) {
    const app = await driver.launch();
    await app.waitFor(
      (name) => document.querySelector("h1")?.textContent === name,
      { timeout: 5_000 },
      PRODUCT_NAME,
    );

    const facts = await app.window();
    assert.equal(facts.title, PRODUCT_NAME);

    const shot = join(artifacts, "window-opens.png");
    await app.snapshot(shot);
    const size = await pngSize(shot);
    assert.ok(size.bytes > 0);
    assert.deepEqual(
      { width: size.width, height: size.height },
      {
        width: Math.round(facts.width * facts.scale),
        height: Math.round(facts.height * facts.scale),
      },
    );

    await app.quit();
    assert.equal(app.isRunning(), false);
  },
};
