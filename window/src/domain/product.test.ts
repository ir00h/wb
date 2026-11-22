import { describe, expect, it } from "vitest";
import { PRODUCT_NAME, windowTitle } from "./product";

describe("windowTitle", () => {
  it("is the product name when no board is open", () => {
    expect(windowTitle(null)).toBe(PRODUCT_NAME);
  });

  it("names the open board before the product", () => {
    expect(windowTitle("Kitchen recipes")).toBe("Kitchen recipes · Whiteboard");
  });
});
