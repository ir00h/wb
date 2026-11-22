import { execFile } from "node:child_process";
import { mkdtemp, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";

const RUN_PREFIX = "whiteboard-e2e-run-";
const OWNER_FILE = "owner.json";
const NO_SUCH_PROCESS = 1;

const run = promisify(execFile);

type Owner = { readonly pid: number; readonly started: string };

async function startOf(pid: number): Promise<string | null> {
  try {
    const started = (
      await run("ps", ["-o", "lstart=", "-p", String(pid)], {
        env: { ...process.env, LC_ALL: "C", TZ: "UTC" },
      })
    ).stdout.trim();
    return started === "" ? null : started;
  } catch (error) {
    if ((error as { code?: unknown }).code === NO_SUCH_PROCESS) {
      return null;
    }
    throw error;
  }
}

function parseOwner(text: string): Owner | null {
  try {
    const value: unknown = JSON.parse(text);
    if (
      typeof value === "object" &&
      value !== null &&
      "pid" in value &&
      "started" in value &&
      typeof value.pid === "number" &&
      typeof value.started === "string"
    ) {
      return { pid: value.pid, started: value.started };
    }
  } catch {
    return null;
  }
  return null;
}

async function abandoned(folder: string): Promise<boolean> {
  const owner = parseOwner(await readFile(join(folder, OWNER_FILE), "utf8").catch(() => ""));
  if (owner === null) {
    return false;
  }
  return (await startOf(owner.pid)) !== owner.started;
}

export type Sweep =
  | { readonly kind: "swept"; readonly removed: readonly string[] }
  | { readonly kind: "processesUnreadable"; readonly reason: string };

function reasonOf(error: unknown): string {
  return error instanceof Error ? (error.message.split("\n")[0] ?? error.message) : String(error);
}

export function sweepReport(sweep: Sweep): readonly string[] {
  switch (sweep.kind) {
    case "swept":
      return sweep.removed.map(
        (folder) => `removed ${folder}, left by a run that no longer exists`,
      );
    case "processesUnreadable":
      return [
        `cannot read the process list (${sweep.reason}), so no folder left by an earlier run was removed`,
      ];
    default: {
      const unknown: never = sweep;
      return unknown;
    }
  }
}

export async function removeAbandonedRuns(): Promise<Sweep> {
  try {
    await startOf(process.pid);
  } catch (error) {
    return { kind: "processesUnreadable", reason: reasonOf(error) };
  }
  const removed: string[] = [];
  for (const name of await readdir(tmpdir())) {
    if (!name.startsWith(RUN_PREFIX)) {
      continue;
    }
    const folder = join(tmpdir(), name);
    if (await abandoned(folder).catch(() => false)) {
      await rm(folder, { recursive: true, force: true });
      removed.push(folder);
    }
  }
  return { kind: "swept", removed };
}

export async function claimRunFolder(): Promise<string> {
  const folder = await mkdtemp(join(tmpdir(), RUN_PREFIX));
  const started = await startOf(process.pid).catch(() => null);
  if (started !== null) {
    const owner: Owner = { pid: process.pid, started };
    await writeFile(join(folder, OWNER_FILE), JSON.stringify(owner));
  }
  return folder;
}
