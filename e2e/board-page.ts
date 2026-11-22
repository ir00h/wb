import type { Driver, Point, RunningApp } from "./driver.ts";
import { type Garden, plantGarden } from "./garden.ts";

export type Box = {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
};

export type PageCard = {
  readonly id: string;
  readonly title: string;
  readonly tags: readonly string[];
  readonly body: string;
  readonly html: string;
  readonly box: Box;
};

export type PageFrame = {
  readonly id: string;
  readonly title: string;
  readonly box: Box;
};

export type PageBoard = {
  readonly frames: readonly PageFrame[];
  readonly cards: readonly PageCard[];
};

async function layoutSettled(app: RunningApp): Promise<void> {
  await app.waitFor(
    () => document.querySelector(".world")?.getAttribute("data-layout") === "settled",
    { timeout: 5_000 },
  );
}

export async function readBoard(app: RunningApp): Promise<PageBoard> {
  await layoutSettled(app);
  return app.evaluate(() => {
    const box = (element: Element) => {
      const rect = element.getBoundingClientRect();
      return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
    };
    const frames = Array.from(document.querySelectorAll("[data-section-body]"), (element) => {
      const id = element.getAttribute("data-section-body") ?? "";
      return {
        id,
        title: element.querySelector(".section-title")?.textContent ?? "",
        box: box(element),
      };
    });
    const cards = Array.from(document.querySelectorAll("[data-card]"), (element) => ({
      id: element.getAttribute("data-card") ?? "",
      title: element.querySelector(".card-title")?.textContent ?? "",
      tags: Array.from(element.querySelectorAll(".card-tags b"), (tag) => tag.textContent ?? ""),
      body: element.querySelector(".card-body")?.textContent ?? "",
      html: element.querySelector(".card-body")?.innerHTML ?? "",
      box: box(element),
    }));
    return { frames, cards };
  });
}

export function inside(outer: Box, inner: Box): boolean {
  return (
    inner.x >= outer.x &&
    inner.y >= outer.y &&
    inner.x + inner.width <= outer.x + outer.width &&
    inner.y + inner.height <= outer.y + outer.height
  );
}

export async function centreOf(app: RunningApp, selector: string): Promise<Point> {
  await layoutSettled(app);
  const point = await app.evaluate((wanted) => {
    const element = document.querySelector(wanted);
    if (element === null) {
      return null;
    }
    const rect = element.getBoundingClientRect();
    return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
  }, selector);
  if (point === null) {
    throw new Error(`nothing on the page matches ${selector}`);
  }
  return point;
}

export function cardsShown(app: RunningApp, count: number): Promise<boolean> {
  return app.waitFor(
    (wanted) => document.querySelectorAll("[data-card]").length === wanted,
    { timeout: 5_000 },
    count,
  );
}

export async function openEditor(app: RunningApp, note: string): Promise<void> {
  await app.click(await centreOf(app, `[data-card="${note}"] .card-title`), 2);
  await app.waitFor(
    (id) => document.activeElement === document.querySelector(`[data-editor="${id}"]`),
    { timeout: 3_000 },
    note,
  );
}

export function editorText(app: RunningApp, note: string): Promise<string | null> {
  return app.evaluate((id) => {
    const editor = document.querySelector(`[data-editor="${id}"]`);
    return editor instanceof HTMLTextAreaElement ? editor.value : null;
  }, note);
}

export async function launchGarden(
  driver: Driver,
): Promise<{ readonly app: RunningApp; readonly garden: Garden }> {
  const planted: { garden?: Garden } = {};
  const app = await driver.launch({
    prepare: async (workspace) => {
      planted.garden = await plantGarden(workspace);
    },
  });
  if (planted.garden === undefined) {
    throw new Error("the garden was not planted before the app started");
  }
  await cardsShown(app, 4);
  return { app, garden: planted.garden };
}

export async function eventually(check: () => Promise<boolean>, timeout: number): Promise<boolean> {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await check()) {
      return true;
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  return check();
}
