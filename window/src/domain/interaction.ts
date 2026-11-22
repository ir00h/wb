import type { NoteId } from "../bindings/NoteId";

export type Interaction =
  | { readonly kind: "idle" }
  | { readonly kind: "editing"; readonly note: NoteId };

export type InputEvent =
  | { readonly type: "doubleClicked"; readonly note: NoteId }
  | { readonly type: "escape" }
  | { readonly type: "blur" }
  | { readonly type: "boardReplaced" }
  | { readonly type: "noteRemoved"; readonly note: NoteId }
  | { readonly type: "editorKept"; readonly note: NoteId };

export type InteractionEffect =
  | { readonly type: "editorOpened"; readonly note: NoteId }
  | { readonly type: "editorClosed"; readonly note: NoteId };

export type Step = {
  readonly state: Interaction;
  readonly effects: readonly InteractionEffect[];
};

export const IDLE: Interaction = { kind: "idle" };

const stay = (state: Interaction): Step => ({ state, effects: [] });

function leave(state: Interaction): Step {
  switch (state.kind) {
    case "idle":
      return stay(state);
    case "editing":
      return { state: IDLE, effects: [{ type: "editorClosed", note: state.note }] };
    default: {
      const unknown: never = state;
      return unknown;
    }
  }
}

function edit(state: Interaction, note: NoteId): Step {
  if (state.kind === "editing" && state.note === note) {
    return stay(state);
  }
  const left = leave(state);
  return {
    state: { kind: "editing", note },
    effects: [...left.effects, { type: "editorOpened", note }],
  };
}

export function step(state: Interaction, event: InputEvent): Step {
  switch (event.type) {
    case "doubleClicked":
      return edit(state, event.note);
    case "editorKept":
      return state.kind === "editing" && state.note === event.note
        ? stay(state)
        : { state: { kind: "editing", note: event.note }, effects: [] };
    case "escape":
    case "blur":
    case "boardReplaced":
      return leave(state);
    case "noteRemoved":
      return state.kind === "editing" && state.note === event.note ? leave(state) : stay(state);
    default: {
      const unknown: never = event;
      return unknown;
    }
  }
}
