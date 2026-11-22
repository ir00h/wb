import { type FocusEvent, type KeyboardEvent, type MouseEvent, useCallback } from "react";
import type { NoteId } from "../bindings/NoteId";
import { edit, input, useWindow } from "../state/store";

function keepFocus(event: MouseEvent) {
  event.preventDefault();
}

function leftTheCard(event: FocusEvent<HTMLElement>): boolean {
  const card = event.currentTarget.closest("[data-card]");
  const next = event.relatedTarget;
  return !(next instanceof Node && card?.contains(next));
}

export function CardEditor({ note }: { note: NoteId }) {
  const session = useWindow((state) => state.sessions.get(note));
  const focusOnOpen = useCallback((element: HTMLTextAreaElement | null) => {
    if (element !== null && document.activeElement !== element) {
      element.focus();
      element.setSelectionRange(element.value.length, element.value.length);
    }
  }, []);
  if (session === undefined) {
    return null;
  }
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      input({ type: "escape" });
    }
  };
  const onBlur = (event: FocusEvent<HTMLElement>) => {
    if (leftTheCard(event)) {
      input({ type: "blur" });
    }
  };
  const phase = session.phase;
  return (
    <div className="card-edit">
      <textarea
        ref={focusOnOpen}
        className="card-editor"
        data-editor={note}
        value={session.draft}
        spellCheck={false}
        onChange={(event) => edit(note, { type: "typed", draft: event.target.value })}
        onKeyDown={onKeyDown}
        onBlur={onBlur}
      />
      {phase.kind === "conflict" && (
        <div className="conflict" data-conflict={note}>
          <p className="conflict-label">Another program changed this note. On disk now:</p>
          <p className="conflict-theirs" data-theirs>
            {phase.theirs}
          </p>
          <div className="conflict-actions">
            <button
              type="button"
              data-keep-mine
              onMouseDown={keepFocus}
              onClick={() => edit(note, { type: "keepMine" })}
            >
              Keep mine
            </button>
            <button
              type="button"
              data-take-theirs
              onMouseDown={keepFocus}
              onClick={() => edit(note, { type: "takeTheirs" })}
            >
              Take theirs
            </button>
          </div>
        </div>
      )}
      {(phase.kind === "removed" || phase.kind === "restoring") && (
        <div className="conflict" data-removed={note}>
          <p className="conflict-label">
            Another program removed this note. Your text is still here and is not saved.
          </p>
          <div className="conflict-actions">
            <button
              type="button"
              data-restore
              disabled={phase.kind === "restoring"}
              onMouseDown={keepFocus}
              onClick={() => edit(note, { type: "restore" })}
            >
              Save as a new note
            </button>
            <button
              type="button"
              data-discard
              disabled={phase.kind === "restoring"}
              onMouseDown={keepFocus}
              onClick={() => edit(note, { type: "discard" })}
            >
              Discard
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
