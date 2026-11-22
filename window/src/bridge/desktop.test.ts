import { describe, expect, it } from "vitest";
import { parseFailure } from "./desktop";

describe("parseFailure", () => {
  it("reads the kind of the core's error and its sentence, and nothing else", () => {
    const rejection = {
      error: { type: "boardNotFound", board: "kitchen" },
      message: "there is no board `kitchen` in this workspace",
    };
    expect(parseFailure("openBoard", rejection)).toEqual({
      kind: "boardNotFound",
      message: "there is no board `kitchen` in this workspace",
    });
  });

  it("turns anything else into a call the app did not understand", () => {
    expect(parseFailure("apply", "the IPC channel closed")).toEqual({
      kind: "badCall",
      message: "the app answered apply in a form the window does not know: the IPC channel closed",
    });
    expect(parseFailure("apply", { error: { type: "eaten" }, message: "x" }).kind).toBe("badCall");
    expect(parseFailure("apply", { error: { type: "busy" } }).kind).toBe("badCall");
    expect(parseFailure("apply", { error: "busy", message: "x" }).kind).toBe("badCall");
  });
});
