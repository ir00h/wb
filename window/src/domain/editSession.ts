import type { NoteId } from "../bindings/NoteId";

export type EditPhase =
  | { readonly kind: "clean" }
  | { readonly kind: "dirty" }
  | { readonly kind: "saving"; readonly sent: string }
  | { readonly kind: "conflict"; readonly theirs: string };

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
  | { readonly type: "removed" }
  | { readonly type: "keepMine" }
  | { readonly type: "takeTheirs" };

export type EditEffect =
  | { readonly type: "send"; readonly note: NoteId; readonly base: string; readonly to: string }
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
      return unchanged({ ...session, draft });
    default: {
      const unknown: never = phase;
      return unknown;
    }
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
      return body === session.draft
        ? settled({ ...session, base: body, phase: CLEAN })
        : unchanged({ ...session, phase: { kind: "conflict", theirs: body } });
    default: {
      const unknown: never = phase;
      return unknown;
    }
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
      return { session, effects: [{ type: "stayOpen" }] };
    default: {
      const unknown: never = phase;
      return unknown;
    }
  }
}

const LOST_TEXT = "Another program removed this note before your text was saved.";

function removedEffects(session: EditSession): readonly EditEffect[] {
  const end: EditEffect = { type: "end" };
  if (session.draft === session.base) {
    return [end];
  }
  return [end, { type: "notice", text: LOST_TEXT }];
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
      return session.phase.kind === "saving"
        ? {
            session: { ...session, open: true, phase: DIRTY },
            effects: [{ type: "stayOpen" }, { type: "notice", text: event.message }],
          }
        : unchanged(session);
    case "changedOutside":
      return changedOutside(session, event.body);
    case "removed":
      return { session, effects: removedEffects(session) };
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
    default: {
      const unknown: never = event;
      return unknown;
    }
  }
}
