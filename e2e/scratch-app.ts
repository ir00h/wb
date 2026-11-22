import { mkdir, rm, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { parseArgs } from "node:util";
import { backgroundRequested, ChannelDriver } from "./channel.ts";
import type { RunningApp } from "./driver.ts";
import { BOARD, type Garden, plantGarden } from "./garden.ts";
import { claimRunFolder, removeAbandonedRuns, sweepReport } from "./scratch.ts";

const WATCH_MS = 200;
const USAGE =
  "usage: node e2e/scratch-app.ts [--garden] [--session <file>]\n" +
  "  starts the app built with the automation feature on a new scratch workspace and keeps it running\n" +
  "  --garden          plant the garden board with the built wb before the app starts\n" +
  "  --session <file>  also write the session line to this file\n" +
  "  the app is target/debug/whiteboard-automation and wb is target/debug/wb unless WB_E2E_SHELL and WB_E2E_WB say otherwise";

type Session = {
  readonly address: string;
  readonly appPid: number | null;
  readonly runnerPid: number;
  readonly runFolder: string;
  readonly workspace: string;
  readonly board: string | null;
  readonly sections: Garden["sections"] | null;
  readonly notes: Garden["notes"] | null;
  readonly background: boolean;
  readonly shell: string;
  readonly wb: string;
};

function options(): { readonly garden: boolean; readonly session: string | null } {
  try {
    const { values } = parseArgs({
      options: {
        garden: { type: "boolean", default: false },
        session: { type: "string" },
      },
      strict: true,
    });
    return {
      garden: values.garden,
      session: values.session === undefined ? null : resolve(values.session),
    };
  } catch (error) {
    console.error(`${error instanceof Error ? error.message : String(error)}\n${USAGE}`);
    process.exit(2);
  }
}

function untilEnded(app: RunningApp, stopRequested: () => boolean): Promise<void> {
  return new Promise((done) => {
    const timer = setInterval(() => {
      if (stopRequested() || !app.isRunning()) {
        clearInterval(timer);
        done();
      }
    }, WATCH_MS);
  });
}

const chosen = options();
const shell = resolve(process.env.WB_E2E_SHELL ?? "target/debug/whiteboard-automation");
const wb = resolve(process.env.WB_E2E_WB ?? "target/debug/wb");
process.env.WB_E2E_WB = wb;

for (const line of sweepReport(await removeAbandonedRuns())) {
  console.error(line);
}

const runFolder = await claimRunFolder();
const background = backgroundRequested();
const driver = new ChannelDriver(shell, background, runFolder);
let stopRequested = false;
for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.once(signal, () => {
    stopRequested = true;
  });
}

let exitCode = 0;
try {
  const planted: { garden?: Garden } = {};
  const app = await driver.launch({
    prepare: async (workspace) => {
      if (chosen.garden) {
        planted.garden = await plantGarden(workspace);
      }
    },
  });
  const session: Session = {
    address: app.address,
    appPid: app.pid,
    runnerPid: process.pid,
    runFolder,
    workspace: app.workspace,
    board: planted.garden === undefined ? null : BOARD,
    sections: planted.garden?.sections ?? null,
    notes: planted.garden?.notes ?? null,
    background,
    shell,
    wb,
  };
  const line = JSON.stringify(session);
  if (chosen.session !== null) {
    await mkdir(dirname(chosen.session), { recursive: true });
    await writeFile(chosen.session, `${line}\n`);
  }
  console.log(line);
  await untilEnded(app, () => stopRequested);
  console.error(app.isRunning() ? "stopping the app" : "the app has ended");
} catch (error) {
  exitCode = 1;
  console.error(error instanceof Error ? error.message : String(error));
} finally {
  try {
    await driver.dispose();
  } catch (error) {
    exitCode = 1;
    console.error(error instanceof Error ? error.message : String(error));
  }
  await rm(runFolder, { recursive: true, force: true });
  console.error(`removed ${runFolder}`);
}
process.exitCode = exitCode;
