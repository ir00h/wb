import { describe, expect, it } from "vitest";
import { parseFailure } from "./desktop";

describe("parseFailure", () => {
  it("reads the core's error and its sentence", () => {
    const rejection = {
      error: { type: "boardNotFound", board: "kitchen" },
      message: "there is no board `kitchen` in this workspace",
    };
    expect(parseFailure("openBoard", rejection)).toEqual(rejection);
  });

  it("turns anything else into a call the app did not understand", () => {
    expect(parseFailure("apply", "the IPC channel closed")).toEqual({
      error: {
        type: "badCall",
        call: "apply",
        message:
          "the app answered apply in a form the window does not know: the IPC channel closed",
      },
      message: "the app answered apply in a form the window does not know: the IPC channel closed",
    });
    expect(parseFailure("apply", { error: { type: "eaten" }, message: "x" }).error.type).toBe(
      "badCall",
    );
  });
});
