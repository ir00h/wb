import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { readdir, readFile, writeFile } from "node:fs/promises";
import { join, relative } from "node:path";
import { promisify } from "node:util";

const run = promisify(execFile);

export const BOARD = "garden";

export function wbPath(): string {
  const wb = process.env.WB_E2E_WB;
  if (wb === undefined) {
    throw new Error("set WB_E2E_WB to the wb binary of this workspace");
  }
  return wb;
}

export async function wb(workspace: string, ...args: readonly string[]): Promise<string> {
  try {
    const { stdout } = await run(wbPath(), ["--workspace", workspace, "--board", BOARD, ...args]);
    return stdout;
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    throw new Error(`wb ${args.join(" ")} failed: ${reason}`);
  }
}

async function json(workspace: string, ...args: readonly string[]): Promise<unknown> {
  return JSON.parse(await wb(workspace, ...args, "--json"));
}

function idOf(value: unknown): string {
  if (
    typeof value === "object" &&
    value !== null &&
    "id" in value &&
    typeof value.id === "string"
  ) {
    return value.id;
  }
  throw new Error(`wb answered without an id: ${JSON.stringify(value)}`);
}

export type Garden = {
  readonly workspace: string;
  readonly sections: Readonly<Record<"beds" | "raised" | "harvest", string>>;
  readonly notes: Readonly<Record<"tomato" | "plants" | "peas" | "swap", string>>;
};

export const TOMATO_BODY =
  "Six plants along the south fence. Stake them in June.\n\nWater at the root, never the leaves.";

export async function addNote(
  workspace: string,
  title: string,
  section: string,
  tags: readonly string[],
  body: string,
): Promise<string> {
  const tagArgs = tags.flatMap((tag) => ["--tag", tag]);
  return idOf(
    await json(
      workspace,
      "note",
      "add",
      "--title",
      title,
      "--section",
      section,
      ...tagArgs,
      "--body",
      body,
    ),
  );
}

export async function colourTags(
  workspace: string,
  hues: Readonly<Record<string, number>>,
): Promise<void> {
  const path = join(workspace, BOARD, "board.config.json");
  const config: unknown = JSON.parse(await readFile(path, "utf8"));
  if (typeof config !== "object" || config === null) {
    throw new Error("board.config.json is not an object");
  }
  const tags = Object.entries(hues).map(([name, hue]) => ({ name, hue }));
  await writeFile(path, `${JSON.stringify({ ...config, tags }, null, 2)}\n`);
}

export async function plantGarden(workspace: string): Promise<Garden> {
  await run(wbPath(), ["--workspace", workspace, "board", "add", "Garden"]);
  const beds = idOf(
    await json(workspace, "section", "add", "Beds", "--note", "What grows where", "--hue", "120"),
  );
  const raised = idOf(
    await json(workspace, "section", "add", "Raised beds", "--parent", "Beds", "--hue", "210"),
  );
  const harvest = idOf(await json(workspace, "section", "add", "Harvest", "--hue", "40"));
  await colourTags(workspace, { vegetable: 120, herb: 200 });
  const tomato = await addNote(
    workspace,
    "Tomato plan for the south fence",
    "Beds / Raised beds",
    ["vegetable", "chore"],
    TOMATO_BODY,
  );
  const plants = await addNote(
    workspace,
    "Plant list",
    "Beds",
    ["plan"],
    "Tomatoes, basil and beans.",
  );
  const peas = await addNote(
    workspace,
    "Pick peas",
    "Harvest",
    ["vegetable"],
    "June, in the morning.\n\n- [x] Basket\n- [ ] Gloves",
  );
  const swap = await addNote(
    workspace,
    "Seed swap list",
    "none",
    ["done"],
    'Seeds to bring: **beans** and peas.\n\n<img src="x" onerror="window.__ran = true">',
  );
  return {
    workspace,
    sections: { beds, raised, harvest },
    notes: { tomato, plants, peas, swap },
  };
}

export async function treeHashes(folder: string): Promise<ReadonlyMap<string, string>> {
  const hashes = new Map<string, string>();
  const visit = async (current: string) => {
    for (const entry of await readdir(current, { withFileTypes: true })) {
      const path = join(current, entry.name);
      const name = relative(folder, path);
      if (entry.isDirectory()) {
        if (entry.name !== ".locks") {
          await visit(path);
        }
      } else {
        hashes.set(
          name,
          createHash("sha256")
            .update(await readFile(path))
            .digest("hex"),
        );
      }
    }
  };
  await visit(folder);
  return hashes;
}

export function changedFiles(
  before: ReadonlyMap<string, string>,
  after: ReadonlyMap<string, string>,
): readonly string[] {
  const names = new Set([...before.keys(), ...after.keys()]);
  return [...names].filter((name) => before.get(name) !== after.get(name)).sort();
}
