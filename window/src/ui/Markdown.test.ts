import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Markdown } from "./Markdown";

const HOSTILE = [
  "Before <b>bold</b> after.",
  "",
  '<img src="x" onerror="window.__ran = true">',
  "",
  "<script>window.__ran = true</script>",
  "",
  '<div style="position: fixed; inset: 0">cover</div>',
  "",
  "[link](javascript:window.__ran=true)",
].join("\n");

describe("Markdown", () => {
  it("draws no raw HTML from a note's body", () => {
    const html = renderToStaticMarkup(createElement(Markdown, { text: HOSTILE }));
    expect(html).not.toMatch(/<(b|img|script|div style)\b/);
    expect(html).not.toContain("onerror");
    expect(html).not.toContain("javascript:");
    expect(html).toContain("Before");
  });
});
