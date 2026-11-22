import { describe, expect, it } from "vitest";
import { noteId } from "./boards.fixture";
import {
  beginEditing,
  closeVerdict,
  type EditEvent,
  type EditSession,
  type EditStep,
  KEPT_OPEN_FOR_CONFLICT,
  stepEdit,
} from "./editSession";

const plantList = noteId("n1");

function run(events: readonly EditEvent[], from = beginEditing(plantList, "Tomatoes")): EditStep {
  let current: EditStep = { session: from, effects: [] };
  for (const event of events) {
    current = stepEdit(current.session, event);
  }
  return current;
}

const sends = (step: EditStep) => step.effects.filter((effect) => effect.type === "send");

describe("edit session", () => {
  it("saves typed text later, based on the text it started from", () => {
    expect(run([{ type: "typed", draft: "Tomatoes, beans" }]).effects).toEqual([
      { type: "saveLater" },
    ]);
    expect(sends(run([{ type: "typed", draft: "Tomatoes, beans" }, { type: "saveDue" }]))).toEqual([
      { type: "send", note: plantList, base: "Tomatoes", to: "Tomatoes, beans" },
    ]);
  });

  it("keeps one operation in flight and sends the rest after it lands", () => {
    const inFlight = run([
      { type: "typed", draft: "Tomatoes, beans" },
      { type: "saveDue" },
      { type: "typed", draft: "Tomatoes, beans, peas" },
      { type: "saveDue" },
    ]);
    expect(sends(inFlight)).toEqual([]);
    expect(inFlight.session.phase).toEqual({ kind: "saving", sent: "Tomatoes, beans" });
    const landed = stepEdit(inFlight.session, { type: "applied", body: "Tomatoes, beans" });
    expect(landed.session.phase).toEqual({ kind: "dirty" });
    expect(landed.session.base).toBe("Tomatoes, beans");
    expect(sends(stepEdit(landed.session, { type: "saveDue" }))[0]).toMatchObject({
      base: "Tomatoes, beans",
      to: "Tomatoes, beans, peas",
    });
  });

  it("saves when the editor closes and ends once the save lands", () => {
    const closed = run([{ type: "typed", draft: "Beans" }, { type: "closed" }]);
    expect(sends(closed)).toHaveLength(1);
    expect(stepEdit(closed.session, { type: "applied", body: "Beans" }).effects).toEqual([
      { type: "end" },
    ]);
  });

  it("follows an outside change when nothing was typed", () => {
    const followed = run([{ type: "changedOutside", body: "Potatoes" }]);
    expect(followed.session).toMatchObject({ base: "Potatoes", draft: "Potatoes" });
  });

  it("never replaces typed text with an outside change", () => {
    const clash = run([
      { type: "typed", draft: "Tomatoes and basil" },
      { type: "changedOutside", body: "Potatoes" },
    ]);
    expect(clash.session.draft).toBe("Tomatoes and basil");
    expect(clash.session.phase).toEqual({ kind: "conflict", theirs: "Potatoes" });
    expect(clash.effects).toContainEqual({ type: "cancelSave" });
    const refused = run([
      { type: "typed", draft: "Tomatoes and basil" },
      { type: "saveDue" },
      { type: "conflict", current: "Potatoes" },
    ]);
    expect(refused.session.draft).toBe("Tomatoes and basil");
    expect(refused.session.phase).toEqual({ kind: "conflict", theirs: "Potatoes" });
  });

  it("keeps the editor open on a conflict, and sends mine based on theirs", () => {
    const inConflict: EditSession = run([
      { type: "typed", draft: "Mine" },
      { type: "changedOutside", body: "Theirs" },
    ]).session;
    expect(stepEdit(inConflict, { type: "closed" }).effects).toEqual([{ type: "stayOpen" }]);
    expect(sends(stepEdit(inConflict, { type: "keepMine" }))).toEqual([
      { type: "send", note: plantList, base: "Theirs", to: "Mine" },
    ]);
    expect(stepEdit(inConflict, { type: "takeTheirs" }).session).toMatchObject({
      base: "Theirs",
      draft: "Theirs",
      phase: { kind: "clean" },
    });
  });

  it("says so when the note is removed under unsaved text", () => {
    expect(run([{ type: "typed", draft: "Mine" }, { type: "removed" }]).effects).toEqual([
      { type: "end" },
      { type: "notice", text: "Another program removed this note before your text was saved." },
    ]);
  });
});

describe("closing the window", () => {
  const typed = run([{ type: "typed", draft: "Tomatoes, beans" }]).session;
  const saving = stepEdit(typed, { type: "saveDue" }).session;
  const clean = beginEditing(noteId("n2"), "Peas");
  const conflicted = stepEdit(saving, { type: "conflict", current: "Potatoes" }).session;

  it("leaves when nothing typed is unsaved", () => {
    expect(closeVerdict([])).toEqual({ kind: "leave" });
    expect(closeVerdict([clean])).toEqual({ kind: "leave" });
  });

  it("sends typed text that waits for its pause, then waits for the answer", () => {
    expect(closeVerdict([clean, typed])).toEqual({ kind: "send", notes: [plantList] });
    expect(closeVerdict([saving, typed])).toEqual({ kind: "wait" });
  });

  it("stays open while a conflict holds typed text", () => {
    expect(closeVerdict([clean, conflicted])).toEqual({
      kind: "stay",
      reason: KEPT_OPEN_FOR_CONFLICT,
    });
  });
});
