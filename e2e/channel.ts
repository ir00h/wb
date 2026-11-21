import { type ChildProcess, execFile, spawn } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { promisify } from "node:util";
import type {
  Driver,
  Files,
  LaunchOptions,
  PageFunction,
  RunningApp,
  WaitOptions,
  WindowFacts,
} from "./driver.ts";

const LISTENING = /^AUTOMATION_LISTENING (\S+)$/m;
const LAUNCH_TIMEOUT_MS = 60_000;
const QUIT_TIMEOUT_MS = 5_000;
const KILL_GRACE_MS = 3_000;
const DEFAULT_WAIT_MS = 5_000;
const POLL_MS = 50;
const FRONTMOST_SAMPLE_MS = 100;

const run = promisify(execFile);
const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export function backgroundRequested(): boolean {
  return process.env.WB_E2E_FOREGROUND !== "1";
}

class ChannelFailure extends Error {}

function tookFocus(samples: number): string {
  return `the app became the frontmost app in background mode (${samples} samples); set the activation policy before the event loop starts`;
}

async function seed(folder: string, files: Files): Promise<void> {
  for (const [relative, content] of Object.entries(files)) {
    const path = join(folder, relative);
    await mkdir(dirname(path), { recursive: true });
    await writeFile(path, content);
  }
}

function exited(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) {
    return Promise.resolve();
  }
  return new Promise((resolve) => child.once("exit", () => resolve()));
}

async function stop(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) {
    return;
  }
  child.kill("SIGTERM");
  const outcome = await Promise.race([
    exited(child).then(() => "exited"),
    sleep(KILL_GRACE_MS).then(() => "lingering"),
  ]);
  if (outcome === "lingering") {
    child.kill("SIGKILL");
    await exited(child);
  }
}

function announcedAddress(child: ChildProcess, output: () => string): Promise<string> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new ChannelFailure(`the app did not announce its channel:\n${output()}`)),
      LAUNCH_TIMEOUT_MS,
    );
    const look = () => {
      const match = LISTENING.exec(output());
      if (match?.[1] !== undefined) {
        clearTimeout(timer);
        resolve(match[1]);
      }
    };
    child.stdout?.on("data", look);
    child.once("exit", (code, signal) => {
      clearTimeout(timer);
      reject(
        new ChannelFailure(
          `the app exited (${code ?? signal}) before it announced its channel:\n${output()}`,
        ),
      );
    });
  });
}

async function frontmostPid(): Promise<number | null> {
  try {
    const front = (await run("lsappinfo", ["front"])).stdout.trim();
    const info = (await run("lsappinfo", ["info", "-only", "pid", front])).stdout;
    const match = /"pid"=(\d+)/.exec(info);
    return match?.[1] === undefined ? null : Number(match[1]);
  } catch {
    return null;
  }
}

function watchFocus(pid: number): () => number {
  let samplesInFront = 0;
  let watching = true;
  const loop = async () => {
    while (watching) {
      if ((await frontmostPid()) === pid) {
        samplesInFront += 1;
      }
      await sleep(FRONTMOST_SAMPLE_MS);
    }
  };
  void loop();
  return () => {
    watching = false;
    return samplesInFront;
  };
}

class ChannelApp implements RunningApp {
  readonly workspace: string;
  readonly #base: string;
  readonly #child: ChildProcess;
  readonly #stopWatchingFocus: (() => number) | null;

  constructor(
    workspace: string,
    base: string,
    child: ChildProcess,
    stopWatchingFocus: (() => number) | null,
  ) {
    this.workspace = workspace;
    this.#base = base;
    this.#child = child;
    this.#stopWatchingFocus = stopWatchingFocus;
  }

  async #request(method: "GET" | "POST", route: string, body?: string): Promise<Response> {
    const response = await fetch(
      `${this.#base}${route}`,
      body === undefined ? { method } : { method, body },
    );
    if (response.status === 422) {
      throw new Error(`the script threw in the web view: ${await response.text()}`);
    }
    if (!response.ok) {
      throw new ChannelFailure(
        `${method} ${route} failed (${response.status}): ${await response.text()}`,
      );
    }
    return response;
  }

  async evaluate<Args extends readonly unknown[], Result>(
    fn: PageFunction<Args, Result>,
    ...args: Args
  ): Promise<Result> {
    const script = `return (${fn.toString()})(...${JSON.stringify(args)});`;
    const response = await this.#request("POST", "/evaluate", script);
    return (await response.json()) as Result;
  }

  async waitFor<Args extends readonly unknown[], Result>(
    fn: PageFunction<Args, Result>,
    options: WaitOptions,
    ...args: Args
  ): Promise<Result> {
    const timeout = options.timeout ?? DEFAULT_WAIT_MS;
    const deadline = Date.now() + timeout;
    let last: unknown;
    while (Date.now() < deadline) {
      try {
        const value = await this.evaluate(fn, ...args);
        if (value) {
          return value;
        }
        last = value;
      } catch (error) {
        if (error instanceof ChannelFailure) {
          throw error;
        }
        last = error instanceof Error ? error.message : error;
      }
      await sleep(POLL_MS);
    }
    throw new Error(
      `timed out after ${timeout} ms waiting for ${fn.toString()} with ${JSON.stringify(args)}; last value: ${JSON.stringify(last)}`,
    );
  }

  async snapshot(path: string): Promise<void> {
    const response = await this.#request("GET", "/snapshot");
    await mkdir(dirname(path), { recursive: true });
    await writeFile(path, new Uint8Array(await response.arrayBuffer()));
  }

  async window(): Promise<WindowFacts> {
    const response = await this.#request("GET", "/window");
    return (await response.json()) as WindowFacts;
  }

  async quit(): Promise<void> {
    await this.#request("POST", "/quit");
    const outcome = await Promise.race([
      exited(this.#child).then(() => "exited"),
      sleep(QUIT_TIMEOUT_MS).then(() => "running"),
    ]);
    if (outcome === "running") {
      throw new Error(`the app was still running ${QUIT_TIMEOUT_MS} ms after it was asked to quit`);
    }
    this.#checkFocus();
  }

  isRunning(): boolean {
    if (this.#child.exitCode !== null || this.#child.signalCode !== null) {
      return false;
    }
    const pid = this.#child.pid;
    if (pid === undefined) {
      return false;
    }
    try {
      process.kill(pid, 0);
      return true;
    } catch {
      return false;
    }
  }

  #checkFocus(): void {
    const samplesInFront = this.#stopWatchingFocus?.() ?? 0;
    if (samplesInFront > 0) {
      throw new Error(tookFocus(samplesInFront));
    }
  }
}

export class ChannelDriver implements Driver {
  readonly #shell: string;
  readonly #background: boolean;
  readonly #children: ChildProcess[] = [];
  readonly #focusWatchers: (() => number)[] = [];
  readonly #folders: string[] = [];

  constructor(shell: string, background: boolean) {
    this.#shell = shell;
    this.#background = background;
  }

  async launch(options: LaunchOptions = {}): Promise<RunningApp> {
    const root = await mkdtemp(join(tmpdir(), "whiteboard-e2e-"));
    this.#folders.push(root);
    const workspace = join(root, "workspace");
    const data = join(root, "data");
    await mkdir(workspace);
    await mkdir(data);
    await seed(workspace, options.workspace ?? {});
    const child = spawn(this.#shell, [], {
      env: {
        ...process.env,
        HOME: data,
        WHITEBOARD_WORKSPACE: workspace,
        WB_AUTOMATION_PORT: "0",
        WB_AUTOMATION_BACKGROUND: this.#background ? "1" : "0",
        ...options.env,
      },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let output = "";
    const collect = (chunk: Buffer) => {
      output += chunk.toString();
    };
    child.stdout.on("data", collect);
    child.stderr.on("data", collect);
    this.#children.push(child);
    const watching =
      this.#background && process.platform === "darwin" && child.pid !== undefined
        ? watchFocus(child.pid)
        : null;
    if (watching !== null) {
      this.#focusWatchers.push(watching);
    }
    const address = await announcedAddress(child, () => output);
    return new ChannelApp(workspace, `http://${address}`, child, watching);
  }

  async dispose(): Promise<void> {
    await Promise.all(this.#children.map(stop));
    const samplesInFront = this.#focusWatchers.reduce(
      (sum, stopWatching) => sum + stopWatching(),
      0,
    );
    await Promise.all(this.#folders.map((folder) => rm(folder, { recursive: true, force: true })));
    this.#children.length = 0;
    this.#focusWatchers.length = 0;
    this.#folders.length = 0;
    if (samplesInFront > 0) {
      throw new Error(tookFocus(samplesInFront));
    }
  }
}
