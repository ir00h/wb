export type Files = Readonly<Record<string, string>>;

export type LaunchOptions = {
  readonly workspace?: Files;
  readonly env?: Readonly<Record<string, string>>;
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
  evaluate<Args extends readonly unknown[], Result>(
    fn: PageFunction<Args, Result>,
    ...args: Args
  ): Promise<Result>;
  waitFor<Args extends readonly unknown[], Result>(
    fn: PageFunction<Args, Result>,
    options: WaitOptions,
    ...args: Args
  ): Promise<Result>;
  snapshot(path: string): Promise<void>;
  window(): Promise<WindowFacts>;
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
