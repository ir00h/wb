import { describe, expect, it } from "vitest";
import { note, tag } from "./boards.fixture";
import { cardLook } from "./look";

describe("cardLook", () => {
  const hues = [
    { tag: tag("vegetable"), hue: 120 },
    { tag: tag("herb"), hue: 200 },
  ];

  it("takes the hue of the card's first coloured tag", () => {
    expect(cardLook(note("n1", "Basil row", null, ["chore", "herb", "vegetable"]), hues)).toEqual({
      kind: "tinted",
      tag: tag("herb"),
      hue: 200,
    });
  });

  it("is plain when no tag has a colour", () => {
    expect(cardLook(note("n2", "Weed the paths", null, ["chore"]), hues)).toEqual({
      kind: "plain",
    });
  });
});
