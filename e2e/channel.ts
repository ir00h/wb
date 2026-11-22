import { type ChildProcess, execFile, spawn } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { promisify } from "node:util";
import type {
  AppliedCount,
  Driver,
  Files,
  Key,
  LaunchOptions,
  PageFunction,
  Point,
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
const EXIT_GRACE_MS = 2_000;
const OUTPUT_TAIL_LINES = 8;

const run = promisify(execFile);
const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export function backgroundRequested(): boolean {
  return process.env.WB_E2E_FOREGROUND !== "1";
}

class ChannelFailure extends Error {}

function zeroSize(facts: WindowFacts): string {
  return `the window is ${facts.width} x ${facts.height} points, so nothing in it can be seen; give the main window a width and a height in tauri.conf.json`;
}

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

function hasEnded(child: ChildProcess): boolean {
  return child.pid === undefined || child.exitCode !== null || child.signalCode !== null;
}

function exited(child: ChildProcess): Promise<void> {
  if (hasEnded(child)) {
    return Promise.resolve();
  }
  return new Promise((resolve) => child.once("exit", () => resolve()));
}

function ending(child: ChildProcess): string {
  return child.signalCode === null
    ? `exited with code ${child.exitCode}`
    : `was killed by ${child.signalCode}`;
}

function tail(output: string): string {
  const lines = output.trimEnd().split("\n").slice(-OUTPUT_TAIL_LINES);
  return lines.length === 1 && lines[0] === "" ? "" : `\n${lines.join("\n")}`;
}

function startFailure(shell: string, error: NodeJS.ErrnoException): ChannelFailure {
  const reason =
    error.code === "ENOENT"
      ? "no file exists at that path"
      : error.code === "EACCES"
        ? "the file is not executable"
        : error.message;
  return new ChannelFailure(`the app at ${shell} could not be started: ${reason}`);
}

async function stop(child: ChildProcess): Promise<void> {
  if (hasEnded(child)) {
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

function announcedAddress(
  shell: string,
  child: ChildProcess,
  output: () => string,
): Promise<string> {
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
    child.once("error", (error) => {
      clearTimeout(timer);
      reject(startFailure(shell, error));
    });
    child.once("exit", () => {
      clearTimeout(timer);
      reject(
        new ChannelFailure(
          `the app ${ending(child)} before it announced its channel${tail(output())}`,
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
  readonly address: string;
  readonly pid: number | null;
  readonly #base: string;
  readonly #child: ChildProcess;
  readonly #output: () => string;
  readonly #stopWatchingFocus: (() => number) | null;

  constructor(
    workspace: string,
    base: string,
    child: ChildProcess,
    output: () => string,
    stopWatchingFocus: (() => number) | null,
  ) {
    this.workspace = workspace;
    this.address = base;
    this.pid = child.pid ?? null;
    this.#base = base;
    this.#child = child;
    this.#output = output;
    this.#stopWatchingFocus = stopWatchingFocus;
  }

  async #unreachable(request: string, cause: unknown): Promise<ChannelFailure> {
    await Promise.race([exited(this.#child), sleep(EXIT_GRACE_MS)]);
    if (hasEnded(this.#child)) {
      return new ChannelFailure(
        `the app ${ending(this.#child)} during ${request}${tail(this.#output())}`,
      );
    }
    const reason = cause instanceof Error ? cause.message : String(cause);
    return new ChannelFailure(`${request} could not reach the app, which still runs: ${reason}`);
  }

  async #request(method: "GET" | "POST", route: string, body?: string): Promise<Response> {
    let response: Response;
    try {
      response = await fetch(
        `${this.#base}${route}`,
        body === undefined ? { method } : { method, body },
      );
    } catch (error) {
      throw await this.#unreachable(`${method} ${route}`, error);
    }
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

  async typeText(text: string): Promise<void> {
    await this.#request("POST", "/type", text);
  }

  async press(key: Key): Promise<void> {
    await this.#request("POST", "/key", key);
  }

  async click(at: Point, count = 1): Promise<void> {
    await this.#request("POST", "/click", JSON.stringify({ x: at.x, y: at.y, count }));
  }

  async applied(): Promise<readonly AppliedCount[]> {
    const response = await this.#request("GET", "/applied");
    return (await response.json()) as AppliedCount[];
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
    const pid = this.#child.pid;
    if (pid === undefined || hasEnded(this.#child)) {
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
  readonly #scratch: string;
  readonly #children: ChildProcess[] = [];
  readonly #focusWatchers: (() => number)[] = [];
  readonly #folders: string[] = [];

  constructor(shell: string, background: boolean, scratch: string) {
    this.#shell = shell;
    this.#background = background;
    this.#scratch = scratch;
  }

  async launch(options: LaunchOptions = {}): Promise<RunningApp> {
    const root = await mkdtemp(join(this.#scratch, "launch-"));
    this.#folders.push(root);
    const workspace = join(root, "workspace");
    const data = join(root, "data");
    await mkdir(workspace);
    await mkdir(data);
    await seed(workspace, options.workspace ?? {});
    await options.prepare?.(workspace);
    const child = spawn(this.#shell, [], {
      env: {
        ...process.env,
        HOME: data,
        WHITEBOARD_WORKSPACE: workspace,
        WB_AUTOMATION_PORT: "0",
        WB_AUTOMATION_BACKGROUND: this.#background ? "1" : "0",
        WB_AUTOMATION_RUNNER: String(process.pid),
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
    child.on("error", (error) => {
      output += `${error.message}\n`;
    });
    this.#children.push(child);
    const watching =
      this.#background && process.platform === "darwin" && child.pid !== undefined
        ? watchFocus(child.pid)
        : null;
    if (watching !== null) {
      this.#focusWatchers.push(watching);
    }
    const address = await announcedAddress(this.#shell, child, () => output);
    const app = new ChannelApp(workspace, `http://${address}`, child, () => output, watching);
    const facts = await app.window();
    if (facts.width <= 0 || facts.height <= 0) {
      throw new ChannelFailure(zeroSize(facts));
    }
    return app;
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
