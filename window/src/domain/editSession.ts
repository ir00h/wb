import type { NoteCard } from "../bindings/NoteCard";
import type { NoteId } from "../bindings/NoteId";
import { unreachable } from "./exhaustive";

export type EditPhase =
  | { readonly kind: "clean" }
  | { readonly kind: "dirty" }
  | { readonly kind: "saving"; readonly sent: string }
  | { readonly kind: "conflict"; readonly theirs: string }
  | { readonly kind: "removed"; readonly card: NoteCard }
  | { readonly kind: "restoring"; readonly card: NoteCard };

export type EditSession = {
  readonly note: NoteId;
  readonly base: string;
  readonly draft: string;
  readonly open: boolean;
  readonly phase: EditPhase;
};

export type EditEvent =
  | { readonly type: "typed"; readonly draft: string }
  | { readonly type: "saveDue" }
  | { readonly type: "closed" }
  | { readonly type: "applied"; readonly body: string }
  | { readonly type: "conflict"; readonly current: string }
  | { readonly type: "failed"; readonly message: string }
  | { readonly type: "changedOutside"; readonly body: string }
  | { readonly type: "removed"; readonly card: NoteCard }
  | { readonly type: "keepMine" }
  | { readonly type: "takeTheirs" }
  | { readonly type: "restore" }
  | { readonly type: "restored" }
  | { readonly type: "discard" };

export type EditEffect =
  | { readonly type: "send"; readonly note: NoteId; readonly base: string; readonly to: string }
  | { readonly type: "restore"; readonly card: NoteCard; readonly body: string }
  | { readonly type: "saveLater" }
  | { readonly type: "cancelSave" }
  | { readonly type: "stayOpen" }
  | { readonly type: "end" }
  | { readonly type: "notice"; readonly text: string };

export type EditStep = {
  readonly session: EditSession;
  readonly effects: readonly EditEffect[];
};

export function beginEditing(note: NoteId, body: string): EditSession {
  return { note, base: body, draft: body, open: true, phase: { kind: "clean" } };
}

const CLEAN: EditPhase = { kind: "clean" };
const DIRTY: EditPhase = { kind: "dirty" };

const unchanged = (session: EditSession): EditStep => ({ session, effects: [] });

function send(session: EditSession, base: string): EditStep {
  return {
    session: { ...session, base, phase: { kind: "saving", sent: session.draft } },
    effects: [{ type: "send", note: session.note, base, to: session.draft }],
  };
}

function conflict(session: EditSession, theirs: string): EditStep {
  return {
    session: { ...session, open: true, phase: { kind: "conflict", theirs } },
    effects: [{ type: "cancelSave" }, { type: "stayOpen" }],
  };
}

function settled(session: EditSession): EditStep {
  return session.open ? unchanged(session) : { session, effects: [{ type: "end" }] };
}

function typed(session: EditSession, draft: string): EditStep {
  const phase = session.phase;
  switch (phase.kind) {
    case "clean":
    case "dirty":
      return draft === session.base
        ? { session: { ...session, draft, phase: CLEAN }, effects: [{ type: "cancelSave" }] }
        : { session: { ...session, draft, phase: DIRTY }, effects: [{ type: "saveLater" }] };
    case "saving":
    case "conflict":
    case "removed":
    case "restoring":
      return unchanged({ ...session, draft });
    default:
      return unreachable(phase);
  }
}

function applied(session: EditSession, body: string): EditStep {
  if (session.phase.kind !== "saving") {
    return unchanged(session);
  }
  const rest = { ...session, base: body };
  if (session.draft === session.phase.sent) {
    return settled({ ...rest, phase: CLEAN });
  }
  return session.open
    ? { session: { ...rest, phase: DIRTY }, effects: [{ type: "saveLater" }] }
    : send(rest, body);
}

function changedOutside(session: EditSession, body: string): EditStep {
  const phase = session.phase;
  switch (phase.kind) {
    case "clean":
      return body === session.base
        ? unchanged(session)
        : unchanged({ ...session, base: body, draft: body });
    case "dirty":
      if (body === session.base) {
        return unchanged(session);
      }
      if (body === session.draft) {
        return {
          session: { ...session, base: body, phase: CLEAN },
          effects: [{ type: "cancelSave" }],
        };
      }
      return conflict(session, body);
    case "saving":
      return unchanged(session);
    case "conflict":
    case "removed":
      return body === session.draft
        ? settled({ ...session, base: body, phase: CLEAN })
        : unchanged({ ...session, open: true, phase: { kind: "conflict", theirs: body } });
    case "restoring":
      return unchanged(session);
    default:
      return unreachable(phase);
  }
}

function closed(session: EditSession): EditStep {
  const phase = session.phase;
  switch (phase.kind) {
    case "clean":
      return { session: { ...session, open: false }, effects: [{ type: "end" }] };
    case "dirty":
      return send({ ...session, open: false }, session.base);
    case "saving":
      return unchanged({ ...session, open: false });
    case "conflict":
    case "removed":
    case "restoring":
      return { session, effects: [{ type: "stayOpen" }] };
    default:
      return unreachable(phase);
  }
}

function removed(session: EditSession, card: NoteCard): EditStep {
  if (session.draft === session.base || session.phase.kind === "restoring") {
    return { session, effects: [{ type: "end" }] };
  }
  return {
    session: { ...session, open: true, phase: { kind: "removed", card } },
    effects: [{ type: "cancelSave" }, { type: "stayOpen" }],
  };
}

function failed(session: EditSession, message: string): EditStep {
  const notice: EditEffect = { type: "notice", text: message };
  switch (session.phase.kind) {
    case "saving":
      return {
        session: { ...session, open: true, phase: DIRTY },
        effects: [{ type: "stayOpen" }, notice],
      };
    case "restoring":
      return {
        session: { ...session, phase: { kind: "removed", card: session.phase.card } },
        effects: [{ type: "stayOpen" }, notice],
      };
    default:
      return unchanged(session);
  }
}

export function stepEdit(session: EditSession, event: EditEvent): EditStep {
  switch (event.type) {
    case "typed":
      return typed(session, event.draft);
    case "saveDue":
      return session.phase.kind === "dirty" ? send(session, session.base) : unchanged(session);
    case "closed":
      return closed(session);
    case "applied":
      return applied(session, event.body);
    case "conflict":
      return session.phase.kind === "saving"
        ? conflict(session, event.current)
        : unchanged(session);
    case "failed":
      return failed(session, event.message);
    case "changedOutside":
      return changedOutside(session, event.body);
    case "removed":
      return removed(session, event.card);
    case "keepMine":
      return session.phase.kind === "conflict"
        ? send(session, session.phase.theirs)
        : unchanged(session);
    case "takeTheirs":
      return session.phase.kind === "conflict"
        ? {
            session: {
              ...session,
              base: session.phase.theirs,
              draft: session.phase.theirs,
              phase: CLEAN,
            },
            effects: [],
          }
        : unchanged(session);
    case "restore":
      return session.phase.kind === "removed"
        ? {
            session: { ...session, phase: { kind: "restoring", card: session.phase.card } },
            effects: [{ type: "restore", card: session.phase.card, body: session.draft }],
          }
        : unchanged(session);
    case "restored":
      return session.phase.kind === "restoring"
        ? { session, effects: [{ type: "end" }] }
        : unchanged(session);
    case "discard":
      return session.phase.kind === "removed"
        ? { session, effects: [{ type: "end" }] }
        : unchanged(session);
    default:
      return unreachable(event);
  }
}

export type CloseVerdict =
  | { readonly kind: "leave" }
  | { readonly kind: "send"; readonly notes: readonly NoteId[] }
  | { readonly kind: "wait" }
  | { readonly kind: "stay"; readonly reason: string };

export const KEPT_OPEN_FOR_CONFLICT =
  "Whiteboard did not quit: another program changed a note you are editing. Choose Keep mine or Take theirs first.";

export const KEPT_OPEN_FOR_REMOVED =
  "Whiteboard did not quit: another program removed a note that holds your typed text. Save it as a new note or discard it first.";

export function closeVerdict(sessions: Iterable<EditSession>): CloseVerdict {
  const dirty: NoteId[] = [];
  let kept: string | null = null;
  for (const session of sessions) {
    const phase = session.phase;
    switch (phase.kind) {
      case "clean":
        break;
      case "saving":
      case "restoring":
        return { kind: "wait" };
      case "dirty":
        dirty.push(session.note);
        break;
      case "conflict":
        kept ??= KEPT_OPEN_FOR_CONFLICT;
        break;
      case "removed":
        kept ??= KEPT_OPEN_FOR_REMOVED;
        break;
      default:
        unreachable(phase);
    }
  }
  if (dirty.length > 0) {
    return { kind: "send", notes: dirty };
  }
  return kept === null ? { kind: "leave" } : { kind: "stay", reason: kept };
}
