import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen as listenToShell } from "@tauri-apps/api/event";
import type { Calls, Events } from "../bindings/Calls";
import { ERROR_TYPES } from "../bindings/Calls";
import type { CoreError } from "../bindings/CoreError";

const SHELL_COMMAND = "call";

export type CallName = keyof Calls;

export type EventName = keyof Events;

export type Failure = {
  readonly kind: CoreError["type"];
  readonly message: string;
};

export type Answer<K extends CallName> =
  | { readonly kind: "replied"; readonly reply: Calls[K]["reply"] }
  | { readonly kind: "failed"; readonly failure: Failure };

export function runsInDesktopShell(): boolean {
  return isTauri();
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isErrorType(value: unknown): value is CoreError["type"] {
  return ERROR_TYPES.some((type) => type === value);
}

export function parseFailure(call: CallName, rejection: unknown): Failure {
  if (isRecord(rejection) && typeof rejection.message === "string") {
    const kind =
      isRecord(rejection.error) && isErrorType(rejection.error.type)
        ? rejection.error.type
        : "badCall";
    return { kind, message: rejection.message };
  }
  return {
    kind: "badCall",
    message: `the app answered ${call} in a form the window does not know: ${String(rejection)}`,
  };
}

export async function call<K extends CallName>(
  kind: K,
  args: Calls[K]["args"],
): Promise<Answer<K>> {
  try {
    const reply = await invoke<Calls[K]["reply"]>(SHELL_COMMAND, { call: kind, args });
    return { kind: "replied", reply };
  } catch (rejection) {
    return { kind: "failed", failure: parseFailure(kind, rejection) };
  }
}

export function listen<K extends EventName>(
  name: K,
  handler: (payload: Events[K]) => void,
): () => void {
  let stopped = false;
  let stop: (() => void) | null = null;
  void listenToShell<Events[K]>(name, (event) => handler(event.payload)).then((unlisten) => {
    if (stopped) {
      unlisten();
    } else {
      stop = unlisten;
    }
  });
  return () => {
    stopped = true;
    stop?.();
  };
}
