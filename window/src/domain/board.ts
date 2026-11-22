import type { BoardEvent } from "../bindings/BoardEvent";
import type { BoardSnapshot } from "../bindings/BoardSnapshot";
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
