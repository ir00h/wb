import { create } from "zustand";
import type { BoardId } from "../bindings/BoardId";
import type { BoardSnapshot } from "../bindings/BoardSnapshot";
import type { Events } from "../bindings/Calls";
import type { DraftSection } from "../bindings/DraftSection";
import type { NoteCard } from "../bindings/NoteCard";
import type { NoteId } from "../bindings/NoteId";
import type { Outcome } from "../bindings/Outcome";
import { call, listen, runsInDesktopShell } from "../bridge/desktop";
import { applyEvents } from "../domain/board";
import {
  beginEditing,
  closeVerdict,
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
const CLOSE_WAIT_MS = 5_000;
const CLOSE_SEND_ROUNDS = 2;
const UNSAVED_ON_CLOSE = "Whiteboard did not quit, because typed text could not be saved.";
const NO_ANSWER_ON_CLOSE = "Whiteboard did not quit, because a save got no answer in time.";

type BoardUpdate = Events["board"];
type BoardHealth = Events["boardHealth"];

export type BoardState =
  | { readonly kind: "opening" }
  | {
      readonly kind: "open";
      readonly snapshot: BoardSnapshot;
      readonly problem: string | null;
    }
  | { readonly kind: "failed"; readonly message: string }
  | { readonly kind: "preview" };

export type View = { readonly x: number; readonly y: number };

export type Notice =
  | { readonly kind: "message"; readonly text: string }
  | { readonly kind: "notQuit"; readonly text: string };

export type WindowState = {
  readonly board: BoardState;
  readonly interaction: Interaction;
  readonly sessions: ReadonlyMap<NoteId, EditSession>;
  readonly heights: ReadonlyMap<NoteId, number>;
  readonly view: View;
  readonly notice: Notice | null;
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
  useWindow.setState({ notice: { kind: "message", text } });
}

function applyUpdate(update: BoardUpdate): void {
  const board = useWindow.getState().board;
  if (board.kind !== "open" || board.snapshot.board.id !== update.board) {
    return;
  }
  const before = board.snapshot;
  useWindow.setState({ board: { ...board, snapshot: applyEvents(before, update.events) } });
  for (const event of update.events) {
    switch (event.type) {
      case "noteChanged":
        edit(event.note.id, { type: "changedOutside", body: event.note.body });
        break;
      case "noteRemoved": {
        const card = before.notes.find((note) => note.id === event.note);
        if (card !== undefined) {
          edit(event.note, { type: "removed", card });
        }
        if (!useWindow.getState().sessions.has(event.note)) {
          input({ type: "noteRemoved", note: event.note });
        }
        break;
      }
      case "sectionsChanged":
      case "tagsChanged":
      case "straysChanged":
        break;
      default:
        unreachable(event);
    }
  }
}

function applyHealth(health: BoardHealth): void {
  const board = useWindow.getState().board;
  if (board.kind === "open" && board.snapshot.board.id === health.board) {
    useWindow.setState({ board: { ...board, problem: health.problem } });
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

function draftSection(card: NoteCard): DraftSection {
  return card.section === null
    ? { type: "noSection" }
    : { type: "named", section: { type: "id", id: card.section } };
}

async function restore(note: NoteId, card: NoteCard, body: string): Promise<void> {
  const snapshot = openSnapshot();
  if (snapshot === null) {
    return;
  }
  const answer = await call("apply", {
    board: snapshot.board.id,
    op: {
      type: "addNote",
      draft: { title: card.title, body, section: draftSection(card), tags: card.tags, source: "" },
    },
  });
  switch (answer.kind) {
    case "replied":
      if (answer.reply.type === "applied") {
        applyUpdate({
          board: snapshot.board.id,
          events: [{ type: "noteChanged", note: answer.reply.note }],
        });
      }
      edit(note, { type: "restored" });
      break;
    case "failed":
      edit(note, { type: "failed", message: answer.failure.message });
      break;
    default:
      unreachable(answer);
  }
}

function runEditEffect(note: NoteId, effect: EditEffect): void {
  switch (effect.type) {
    case "send":
      cancelSave(note);
      void save(effect.note, effect.base, effect.to);
      break;
    case "restore":
      void restore(note, effect.card, effect.body);
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
      if (openSnapshot()?.notes.some((card) => card.id === note) !== true) {
        input({ type: "noteRemoved", note });
      }
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

function sessionsChange(until: number): Promise<void> {
  return new Promise((resolve) => {
    const done = () => {
      clearTimeout(timer);
      unsubscribe();
      resolve();
    };
    const timer = setTimeout(done, Math.max(0, until - Date.now()));
    const unsubscribe = useWindow.subscribe((state, before) => {
      if (state.sessions !== before.sessions) {
        done();
      }
    });
  });
}

function stayOpen(reason: string): Promise<unknown> {
  const failure = useWindow.getState().notice?.text ?? null;
  const text = failure === null || failure === reason ? reason : `${reason} ${failure}`;
  useWindow.setState({ notice: { kind: "notQuit", text } });
  return call("answerClose", { type: "stay" });
}

export function quitWithoutSaving(): void {
  void call("answerClose", { type: "leave" });
}

let closing = false;

async function answerClose(): Promise<void> {
  if (closing) {
    return;
  }
  closing = true;
  try {
    const deadline = Date.now() + CLOSE_WAIT_MS;
    let rounds = 0;
    for (;;) {
      const verdict = closeVerdict(useWindow.getState().sessions.values());
      switch (verdict.kind) {
        case "leave":
          await call("answerClose", { type: "leave" });
          return;
        case "stay":
          await stayOpen(verdict.reason);
          return;
        case "send":
          if (rounds === CLOSE_SEND_ROUNDS) {
            await stayOpen(UNSAVED_ON_CLOSE);
            return;
          }
          rounds += 1;
          for (const note of verdict.notes) {
            edit(note, { type: "saveDue" });
          }
          break;
        case "wait":
          if (Date.now() >= deadline) {
            await stayOpen(NO_ANSWER_ON_CLOSE);
            return;
          }
          await sessionsChange(deadline);
          break;
        default:
          unreachable(verdict);
      }
    }
  } finally {
    closing = false;
  }
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
  const early: BoardUpdate[] = [];
  let opened = false;
  listen("boardHealth", applyHealth);
  listen("closeRequested", () => {
    void answerClose();
  });
  listen("board", (update) => {
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
      useWindow.setState({
        board: { kind: "open", snapshot: applyEvents(snapshot, events), problem: null },
      });
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
