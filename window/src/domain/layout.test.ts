import { describe, expect, it } from "vitest";
import { board, note, noteId, section } from "./boards.fixture";
import { CARD_WIDTH, contains, type Layout, layout } from "./layout";

const garden = board(
  [section("s1", "Beds", [section("s2", "Raised beds")]), section("s3", "Harvest")],
  [
    note("n1", "Plant list", "s1"),
    note("n2", "Tomato plan", "s2"),
    note("n3", "Pick peas", "s3"),
    note("n4", "Seed swap", null),
    note("n5", "Basil row", "s3"),
    note("n6", "Lost note", "s9"),
  ],
);

function frameOf(placed: Layout, id: string) {
  const frame = placed.frames.find((entry) => entry.section.id === id);
  if (frame === undefined) {
    throw new Error(`no frame for ${id}`);
  }
  return frame;
}

function cardOf(placed: Layout, id: string) {
  const card = placed.cards.find((entry) => entry.note === noteId(id));
  if (card === undefined) {
    throw new Error(`no card for ${id}`);
  }
  return card;
}

describe("layout", () => {
  const placed = layout(garden, new Map([[noteId("n1"), 300]]));

  it("places frames in config order, a nested frame inside its parent", () => {
    expect(placed.frames.map((frame) => frame.section.name)).toEqual([
      "Beds",
      "Raised beds",
      "Harvest",
    ]);
    expect(contains(frameOf(placed, "s1").rect, frameOf(placed, "s2").rect)).toBe(true);
    expect(frameOf(placed, "s2").depth).toBe(1);
    expect(frameOf(placed, "s3").rect.x).toBeGreaterThan(
      frameOf(placed, "s1").rect.x + frameOf(placed, "s1").rect.width,
    );
  });

  it("puts each card inside its own frame and no frame it does not belong to", () => {
    const own: Record<string, readonly string[]> = {
      n1: ["s1"],
      n2: ["s1", "s2"],
      n3: ["s3"],
      n5: ["s3"],
    };
    for (const [id, frames] of Object.entries(own)) {
      const inside = placed.frames
        .filter((frame) => contains(frame.rect, cardOf(placed, id).rect))
        .map((frame) => frame.section.id);
      expect(inside, id).toEqual(frames);
    }
  });

  it("keeps a note without a known section outside every frame", () => {
    for (const id of ["n4", "n6"]) {
      const card = cardOf(placed, id);
      expect(card.section).toBeNull();
      expect(placed.frames.some((frame) => contains(frame.rect, card.rect))).toBe(false);
    }
  });

  it("flows cards in columns of the card width without overlap", () => {
    expect(placed.cards.every((card) => card.rect.width === CARD_WIDTH)).toBe(true);
    expect(cardOf(placed, "n1").rect.height).toBe(300);
    const [first, second] = [cardOf(placed, "n3").rect, cardOf(placed, "n5").rect];
    expect(second.y).toBeGreaterThanOrEqual(first.y + first.height);
  });
});
