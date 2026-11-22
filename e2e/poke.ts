import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import type { Key } from "./driver.ts";

const USAGE = `usage: node e2e/poke.ts <address> <command> [arguments]
  <address> is the address in the line node e2e/scratch-app.ts prints, like http://127.0.0.1:50123
  window                     the window's title, size, scale, visibility and background mode
  doctor                     the window facts and what the page shows, without changing anything
  eval <body>                run <body> as an async function in the page and print its value
  type <text>                type <text> one character at a time as system text input
  key <Escape|Enter|Tab|Backspace>
                             press one key
  click <selector> [count]   click the centre of the first element matching <selector>; count 2 double-clicks
  applied                    how many operations the app has applied, by kind and note
  snapshot <file>            save a PNG of the page
  quit                       quit the app`;

const KEYS: readonly Key[] = ["Escape", "Enter", "Tab", "Backspace"];

class Usage extends Error {}

function isKey(name: string): name is Key {
  return (KEYS as readonly string[]).includes(name);
}

function argument(values: readonly string[], index: number, name: string): string {
  const value = values[index];
  if (value === undefined) {
    throw new Usage(`missing ${name}`);
  }
  return value;
}

async function send(
  base: string,
  method: "GET" | "POST",
  route: string,
  body?: string,
): Promise<Response> {
  const response = await fetch(
    `${base}${route}`,
    body === undefined ? { method } : { method, body },
  );
  if (!response.ok) {
    throw new Error(`${method} ${route} failed (${response.status}): ${await response.text()}`);
  }
  return response;
}

async function evaluate(base: string, body: string): Promise<unknown> {
  return (await send(base, "POST", "/evaluate", body)).json();
}

const PAGE_FACTS = `return {
  title: document.title,
  heading: document.querySelector("h1")?.textContent ?? null,
  cardCount: document.querySelector(".card-count")?.textContent ?? null,
  cards: document.querySelectorAll("[data-card]").length,
  sections: Array.from(document.querySelectorAll(".section-title"), (title) => title.textContent),
  notice: document.querySelector(".notice")?.textContent ?? null,
  editor: document.querySelector("[data-editor]")?.getAttribute("data-editor") ?? null,
  visibility: document.visibilityState,
};`;

function centreOf(selector: string): string {
  return `const element = document.querySelector(${JSON.stringify(selector)});
if (element === null) {
  return null;
}
const rect = element.getBoundingClientRect();
return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };`;
}

function isPoint(value: unknown): value is { x: number; y: number } {
  return (
    typeof value === "object" &&
    value !== null &&
    "x" in value &&
    "y" in value &&
    typeof value.x === "number" &&
    typeof value.y === "number"
  );
}

async function poke(base: string, command: string, rest: readonly string[]): Promise<unknown> {
  switch (command) {
    case "window":
      return (await send(base, "GET", "/window")).json();
    case "doctor":
      return {
        window: await (await send(base, "GET", "/window")).json(),
        page: await evaluate(base, PAGE_FACTS),
      };
    case "eval":
      return evaluate(base, argument(rest, 0, "<body>"));
    case "type": {
      const text = argument(rest, 0, "<text>");
      for (const character of text) {
        await send(base, "POST", "/type", character);
      }
      return { typed: text };
    }
    case "key": {
      const key = argument(rest, 0, "<key>");
      if (!isKey(key)) {
        throw new Usage(`unknown key ${key}`);
      }
      await send(base, "POST", "/key", key);
      return { pressed: key };
    }
    case "click": {
      const selector = argument(rest, 0, "<selector>");
      const count = Number(rest[1] ?? "1");
      if (!Number.isInteger(count) || count < 1) {
        throw new Usage(`the click count must be a whole number from 1, not ${rest[1]}`);
      }
      const point = await evaluate(base, centreOf(selector));
      if (!isPoint(point)) {
        throw new Error(`nothing on the page matches ${selector}`);
      }
      await send(base, "POST", "/click", JSON.stringify({ x: point.x, y: point.y, count }));
      return { clicked: selector, at: point, count };
    }
    case "applied":
      return (await send(base, "GET", "/applied")).json();
    case "snapshot": {
      const file = resolve(argument(rest, 0, "<file>"));
      const response = await send(base, "GET", "/snapshot");
      await mkdir(dirname(file), { recursive: true });
      await writeFile(file, new Uint8Array(await response.arrayBuffer()));
      return { snapshot: file };
    }
    case "quit":
      await send(base, "POST", "/quit");
      return { quit: true };
    default:
      throw new Usage(`unknown command ${command}`);
  }
}

const [address, command, ...rest] = process.argv.slice(2);
try {
  if (address === undefined || command === undefined) {
    throw new Usage("missing <address> or <command>");
  }
  console.log(JSON.stringify(await poke(address.replace(/\/$/, ""), command, rest), null, 2));
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  console.error(error instanceof Usage ? `${message}\n${USAGE}` : message);
  process.exitCode = error instanceof Usage ? 2 : 1;
}
