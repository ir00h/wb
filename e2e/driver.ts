export type Files = Readonly<Record<string, string>>;

export type LaunchOptions = {
  readonly workspace?: Files;
  readonly prepare?: (workspace: string) => Promise<void>;
  readonly env?: Readonly<Record<string, string>>;
};

export type Key = "Escape" | "Enter" | "Tab" | "Backspace";

export type WayOut = "quit" | "close" | "terminate";

export type Point = { readonly x: number; readonly y: number };

export type AppliedCount = {
  readonly kind: "addSection" | "addNote" | "editNote";
  readonly note: string | null;
  readonly count: number;
};

export type WaitOptions = {
  readonly timeout?: number;
};

export type WindowFacts = {
  readonly title: string;
  readonly width: number;
  readonly height: number;
  readonly scale: number;
  readonly visible: boolean;
  readonly windowNumber: number | null;
  readonly background: boolean;
};

export type PageFunction<Args extends readonly unknown[], Result> = (
  ...args: Args
) => Result | Promise<Result>;

export interface RunningApp {
  readonly workspace: string;
  readonly address: string;
  readonly pid: number | null;
  evaluate<Args extends readonly unknown[], Result>(
    fn: PageFunction<Args, Result>,
    ...args: Args
  ): Promise<Result>;
  waitFor<Args extends readonly unknown[], Result>(
    fn: PageFunction<Args, Result>,
    options: WaitOptions,
    ...args: Args
  ): Promise<Result>;
  typeText(text: string): Promise<void>;
  press(key: Key): Promise<void>;
  click(at: Point, count?: number): Promise<void>;
  applied(): Promise<readonly AppliedCount[]>;
  snapshot(path: string): Promise<void>;
  window(): Promise<WindowFacts>;
  askToQuit(way: WayOut): Promise<void>;
  exited(timeout: number): Promise<boolean>;
  quit(): Promise<void>;
  isRunning(): boolean;
}

export interface Driver {
  launch(options?: LaunchOptions): Promise<RunningApp>;
  dispose(): Promise<void>;
}

export type Journey = {
  readonly name: string;
  run(driver: Driver, scratch: string): Promise<void>;
};
