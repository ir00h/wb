import { describe, expect, it } from "vitest";
import type { BoardId } from "../bindings/BoardId";
import { PRODUCT_NAME, windowTitle } from "./product";

describe("windowTitle", () => {
  it("is the product name when no board is open", () => {
    expect(windowTitle(null)).toBe(PRODUCT_NAME);
  });

  it("names the open board before the product", () => {
    expect(windowTitle("kitchen/recipes" as BoardId)).toBe("kitchen/recipes · Whiteboard");
  });
});
