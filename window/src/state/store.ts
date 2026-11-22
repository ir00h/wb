import { create } from "zustand";
import type { BoardId } from "../bindings/BoardId";
import type { BoardSnapshot } from "../bindings/BoardSnapshot";
import type { BoardEventPayload } from "../bindings/Calls";
import type { NoteId } from "../bindings/NoteId";
import type { Outcome } from "../bindings/Outcome";
import { call, listen, runsInDesktopShell } from "../bridge/desktop";
import { applyEvents } from "../domain/board";
import {
  beginEditing,
  type EditEffect,
  type EditEvent,
  type EditSession,
  stepEdit,
} from "../domain/editSession";
import { unreachable } from "../domain/exhaustive";
import {
  IDLE,
  type InputEvent,
  type Interaction,
  type InteractionEffect,
  step,
} from "../domain/interaction";

export const SAVE_PAUSE_MS = 700;

export type BoardState =
  | { readonly kind: "opening" }
  | { readonly kind: "open"; readonly snapshot: BoardSnapshot }
  | { readonly kind: "failed"; readonly message: string }
  | { readonly kind: "preview" };

export type View = { readonly x: number; readonly y: number };

export type WindowState = {
  readonly board: BoardState;
  readonly interaction: Interaction;
  readonly sessions: ReadonlyMap<NoteId, EditSession>;
  readonly heights: ReadonlyMap<NoteId, number>;
  readonly view: View;
  readonly notice: string | null;
};

export const useWindow = create<WindowState>(() => ({
  board: { kind: "opening" },
  interaction: IDLE,
  sessions: new Map(),
  heights: new Map(),
  view: { x: 0, y: 0 },
  notice: null,
}));

const saveTimers = new Map<NoteId, ReturnType<typeof setTimeout>>();

function openSnapshot(): BoardSnapshot | null {
  const board = useWindow.getState().board;
  return board.kind === "open" ? board.snapshot : null;
}

function withSession(note: NoteId, session: EditSession | null): void {
  useWindow.setState((state) => {
    const sessions = new Map(state.sessions);
    if (session === null) {
      sessions.delete(note);
    } else {
      sessions.set(note, session);
    }
    return { sessions };
  });
}

function cancelSave(note: NoteId): void {
  const timer = saveTimers.get(note);
  if (timer !== undefined) {
    clearTimeout(timer);
    saveTimers.delete(note);
  }
}

function showNotice(text: string): void {
  useWindow.setState({ notice: text });
}

function applyUpdate(update: BoardEventPayload): void {
  const snapshot = openSnapshot();
  if (snapshot === null || snapshot.board.id !== update.board) {
    return;
  }
  useWindow.setState({ board: { kind: "open", snapshot: applyEvents(snapshot, update.events) } });
  for (const event of update.events) {
    switch (event.type) {
      case "noteChanged":
        edit(event.note.id, { type: "changedOutside", body: event.note.body });
        break;
      case "noteRemoved":
        edit(event.note, { type: "removed" });
        input({ type: "noteRemoved", note: event.note });
        break;
      case "sectionsChanged":
      case "tagsChanged":
      case "straysChanged":
        break;
      default:
        unreachable(event);
    }
  }
}

async function save(note: NoteId, base: string, to: string): Promise<void> {
  const snapshot = openSnapshot();
  if (snapshot === null) {
    return;
  }
  const answer = await call("apply", {
    board: snapshot.board.id,
    op: { type: "editNote", note, edit: { title: null, body: { type: "set", base, to } } },
  });
  switch (answer.kind) {
    case "replied":
      settle(snapshot.board.id, note, answer.reply);
      break;
    case "failed":
      edit(note, { type: "failed", message: answer.failure.message });
      break;
    default:
      unreachable(answer);
  }
}

function settle(board: BoardId, note: NoteId, outcome: Outcome): void {
  switch (outcome.type) {
    case "applied":
      applyUpdate({ board, events: [{ type: "noteChanged", note: outcome.note }] });
      edit(note, { type: "applied", body: outcome.note.body });
      break;
    case "conflict":
      edit(note, { type: "conflict", current: outcome.current.body });
      break;
    case "sectionAdded":
      break;
    default:
      unreachable(outcome);
  }
}

function runEditEffect(note: NoteId, effect: EditEffect): void {
  switch (effect.type) {
    case "send":
      cancelSave(note);
      void save(effect.note, effect.base, effect.to);
      break;
    case "saveLater":
      cancelSave(note);
      saveTimers.set(
        note,
        setTimeout(() => {
          saveTimers.delete(note);
          edit(note, { type: "saveDue" });
        }, SAVE_PAUSE_MS),
      );
      break;
    case "cancelSave":
      cancelSave(note);
      break;
    case "stayOpen":
      input({ type: "editorKept", note });
      break;
    case "end":
      cancelSave(note);
      withSession(note, null);
      break;
    case "notice":
      showNotice(effect.text);
      break;
    default:
      unreachable(effect);
  }
}

export function edit(note: NoteId, event: EditEvent): void {
  const session = useWindow.getState().sessions.get(note);
  if (session === undefined) {
    return;
  }
  const next = stepEdit(session, event);
  withSession(note, next.session);
  for (const effect of next.effects) {
    runEditEffect(note, effect);
  }
}

function runInteractionEffect(effect: InteractionEffect): void {
  switch (effect.type) {
    case "editorOpened": {
      const session = useWindow.getState().sessions.get(effect.note);
      if (session !== undefined) {
        withSession(effect.note, { ...session, open: true });
        return;
      }
      const card = openSnapshot()?.notes.find((note) => note.id === effect.note);
      if (card !== undefined) {
        withSession(effect.note, beginEditing(effect.note, card.body));
      }
      break;
    }
    case "editorClosed":
      edit(effect.note, { type: "closed" });
      break;
    default:
      unreachable(effect);
  }
}

export function input(event: InputEvent): void {
  const next = step(useWindow.getState().interaction, event);
  useWindow.setState({ interaction: next.state });
  for (const effect of next.effects) {
    runInteractionEffect(effect);
  }
}

export function measured(note: NoteId, height: number): void {
  const heights = useWindow.getState().heights;
  if (heights.get(note) === height) {
    return;
  }
  useWindow.setState({ heights: new Map(heights).set(note, height) });
}

export function pan(dx: number, dy: number): void {
  useWindow.setState((state) => ({ view: { x: state.view.x - dx, y: state.view.y - dy } }));
}

export function dismissNotice(): void {
  useWindow.setState({ notice: null });
}

let started = false;

export async function start(): Promise<void> {
  if (started) {
    return;
  }
  started = true;
  if (!runsInDesktopShell()) {
    useWindow.setState({ board: { kind: "preview" } });
    return;
  }
  const early: BoardEventPayload[] = [];
  let opened = false;
  listen((update) => {
    if (opened) {
      applyUpdate(update);
    } else {
      early.push(update);
    }
  });
  const answer = await call("openBoard", { board: null });
  switch (answer.kind) {
    case "replied": {
      const snapshot = answer.reply;
      const events = early
        .filter((update) => update.board === snapshot.board.id)
        .flatMap((update) => update.events);
      input({ type: "boardReplaced" });
      useWindow.setState({ board: { kind: "open", snapshot: applyEvents(snapshot, events) } });
      opened = true;
      break;
    }
    case "failed":
      useWindow.setState({ board: { kind: "failed", message: answer.failure.message } });
      break;
    default:
      unreachable(answer);
  }
}
