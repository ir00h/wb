import { describe, expect, it } from "vitest";
import type { BoardEvent } from "../bindings/BoardEvent";
import { applyEvent, applyEvents, withKeptCards } from "./board";
import { board, note, noteId, section, tag } from "./boards.fixture";
import { beginEditing, stepEdit } from "./editSession";

describe("applyEvent", () => {
  const before = board([section("s1", "Beds")], [note("n1", "Plant list", "s1")]);

  it("replaces a changed note and adds a new one at the end", () => {
    const changed = note("n1", "Plant list 2027", "s1");
    const added = note("n2", "Seeds", null);
    const after = applyEvents(before, [
      { type: "noteChanged", note: changed },
      { type: "noteChanged", note: added },
    ]);
    expect(after.notes).toEqual([changed, added]);
  });

  it("removes a note and takes new sections, tag colours and strays whole", () => {
    const sections = [section("s1", "Beds"), section("s2", "Harvest")];
    const after = applyEvents(before, [
      { type: "noteRemoved", note: noteId("n1") },
      { type: "sectionsChanged", sections },
      { type: "tagsChanged", tags: [{ tag: tag("herb"), hue: 200 }] },
      { type: "straysChanged", strays: ["loose-page.md"] },
    ]);
    expect(after.notes).toEqual([]);
    expect(after.sections).toEqual(sections);
    expect(after.tags.map((entry) => entry.hue)).toEqual([200]);
    expect(after.strays).toEqual(["loose-page.md"]);
  });

  it("is harmless when an event arrives twice", () => {
    const event: BoardEvent = { type: "noteChanged", note: note("n1", "Plant list 2027", "s1") };
    const once = applyEvent(before, event);
    expect(applyEvent(once, event)).toEqual(once);
  });
});

describe("withKeptCards", () => {
  const plants = note("n1", "Plant list", "s1");
  const shown = board([section("s1", "Beds")], []);

  it("keeps the card of a removed note in view while it holds typed text", () => {
    const removed = { ...beginEditing(plants.id, ""), draft: "Mine" };
    const kept = stepEdit(removed, { type: "removed", card: plants }).session;
    expect(withKeptCards(shown, [kept]).notes).toEqual([plants]);
  });

  it("leaves the board as it is when no removed note holds text", () => {
    expect(withKeptCards(shown, [beginEditing(plants.id, "")])).toBe(shown);
  });
});
