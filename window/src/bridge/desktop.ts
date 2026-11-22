import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen as listenToShell } from "@tauri-apps/api/event";
import type { CallFailure } from "../bindings/CallFailure";
import type { Calls, Events } from "../bindings/Calls";
import { ERROR_TYPES } from "../bindings/Calls";
import type { CoreError } from "../bindings/CoreError";

const SHELL_COMMAND = "call";

export type CallName = keyof Calls;

export type EventName = keyof Events;

export type Answer<K extends CallName> =
  | { readonly kind: "replied"; readonly reply: Calls[K]["reply"] }
  | { readonly kind: "failed"; readonly failure: CallFailure };

export function runsInDesktopShell(): boolean {
  return isTauri();
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isErrorType(value: unknown): value is CoreError["type"] {
  return ERROR_TYPES.some((type) => type === value);
}

function hasKnownErrorType(value: unknown): value is CoreError {
  return isRecord(value) && isErrorType(value.type);
}

export function parseFailure(call: CallName, rejection: unknown): CallFailure {
  if (
    isRecord(rejection) &&
    typeof rejection.message === "string" &&
    hasKnownErrorType(rejection.error)
  ) {
    return { error: rejection.error, message: rejection.message };
  }
  const message = `the app answered ${call} in a form the window does not know: ${String(rejection)}`;
  return { error: { type: "badCall", call, message }, message };
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
