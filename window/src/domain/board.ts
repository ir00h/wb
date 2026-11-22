import type { BoardEvent } from "../bindings/BoardEvent";
import type { BoardSnapshot } from "../bindings/BoardSnapshot";
import type { NoteCard } from "../bindings/NoteCard";
import type { EditSession } from "./editSession";
import { unreachable } from "./exhaustive";

export function applyEvent(snapshot: BoardSnapshot, event: BoardEvent): BoardSnapshot {
  switch (event.type) {
    case "noteChanged": {
      const known = snapshot.notes.some((note) => note.id === event.note.id);
      return {
        ...snapshot,
        notes: known
          ? snapshot.notes.map((note) => (note.id === event.note.id ? event.note : note))
          : [...snapshot.notes, event.note],
      };
    }
    case "noteRemoved":
      return { ...snapshot, notes: snapshot.notes.filter((note) => note.id !== event.note) };
    case "sectionsChanged":
      return { ...snapshot, sections: event.sections };
    case "tagsChanged":
      return { ...snapshot, tags: event.tags };
    case "straysChanged":
      return { ...snapshot, strays: event.strays };
    default:
      return unreachable(event);
  }
}

export function applyEvents(snapshot: BoardSnapshot, events: readonly BoardEvent[]): BoardSnapshot {
  return events.reduce(applyEvent, snapshot);
}

export function withKeptCards(
  snapshot: BoardSnapshot,
  sessions: Iterable<EditSession>,
): BoardSnapshot {
  const kept: NoteCard[] = [];
  for (const session of sessions) {
    const phase = session.phase;
    if (
      (phase.kind === "removed" || phase.kind === "restoring") &&
      !snapshot.notes.some((note) => note.id === phase.card.id)
    ) {
      kept.push(phase.card);
    }
  }
  return kept.length === 0 ? snapshot : { ...snapshot, notes: [...snapshot.notes, ...kept] };
}
