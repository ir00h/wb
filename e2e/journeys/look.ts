import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { launchGarden } from "../board-page.ts";
import type { Journey } from "../driver.ts";

const SPEC = new URL("../look-spec.json", import.meta.url);

type Spec = {
  readonly properties: readonly string[];
  readonly elements: Readonly<Record<string, Readonly<Record<string, string>>>>;
};

function isSpec(value: unknown): value is Spec {
  return (
    typeof value === "object" &&
    value !== null &&
    "properties" in value &&
    Array.isArray(value.properties) &&
    value.properties.every((property: unknown) => typeof property === "string") &&
    "elements" in value &&
    typeof value.elements === "object" &&
    value.elements !== null &&
    Object.values(value.elements).every(
      (styles: unknown) =>
        typeof styles === "object" &&
        styles !== null &&
        Object.values(styles).every((style: unknown) => typeof style === "string"),
    )
  );
}

export type Difference = {
  readonly element: string;
  readonly property: string;
  readonly expected: string;
  readonly actual: string | null;
};

export const look: Journey = {
  name: "the window looks like the product",
  async run(driver, artifacts) {
    const spec: unknown = JSON.parse(await readFile(SPEC, "utf8"));
    if (!isSpec(spec)) {
      throw new Error("e2e/look-spec.json is not a look specification");
    }
    const { app, garden } = await launchGarden(driver);
    const { tomato } = garden.notes;
    const { beds, raised } = garden.sections;
    const card = `[data-card="${tomato}"]`;
    const selectors: Readonly<Record<string, string>> = {
      canvas: ".canvas",
      canvasDots: ".dots",
      body: "body",
      sectionFrame: `[data-section-body="${beds}"]`,
      sectionHeader: `[data-section="${beds}"]`,
      sectionTitle: `[data-section="${beds}"] .section-title`,
      sectionNote: `[data-section="${beds}"] .section-note`,
      nestedSectionFrame: `[data-section-body="${raised}"]`,
      nestedSectionTitle: `[data-section="${raised}"] .section-title`,
      card,
      cardTagRow: `${card} .card-tags`,
      tagChip: `${card} .card-tags b`,
      tagChipColoured: `${card} .card-tags b.coloured`,
      tagChipPlain: `${card} .card-tags b:not(.coloured)`,
      cardTitle: `${card} .card-title`,
      cardBody: `${card} .card-body`,
      cardBodyParagraph: `${card} .card-body p`,
      titleBarName: ".title-bar h1",
      titleBarCount: ".title-bar .card-count",
    };
    const wanted = Object.entries(spec.elements).map(([element, styles]) => {
      const selector = selectors[element];
      if (selector === undefined) {
        throw new Error(`the look journey has no element for ${element} of the specification`);
      }
      return { element, selector, properties: Object.keys(styles) };
    });
    const actual = await app.evaluate(
      (requests) =>
        Object.fromEntries(
          requests.map(({ element, selector, properties }) => {
            const found = document.querySelector(selector);
            const style = found === null ? null : getComputedStyle(found);
            return [
              element,
              Object.fromEntries(
                properties.map((property) => [property, style?.getPropertyValue(property) ?? null]),
              ),
            ];
          }),
        ),
      wanted,
    );
    const rows: Difference[] = [];
    const table: string[] = ["| element | property | expected | actual |", "|---|---|---|---|"];
    for (const [element, styles] of Object.entries(spec.elements)) {
      for (const [property, expected] of Object.entries(styles)) {
        const seen = actual[element]?.[property] ?? null;
        table.push(`| ${element} | ${property} | \`${expected}\` | \`${seen}\` |`);
        if (seen !== expected) {
          rows.push({ element, property, expected, actual: seen });
        }
      }
    }
    await writeFile(join(artifacts, "look-comparison.md"), `${table.join("\n")}\n`);
    await app.snapshot(join(artifacts, "look.png"));
    assert.deepEqual(
      rows,
      [],
      `the computed styles differ from e2e/look-spec.json:\n${rows
        .map(
          (row) => `  ${row.element} ${row.property}: expected ${row.expected}, got ${row.actual}`,
        )
        .join("\n")}`,
    );
    await app.quit();
  },
};
