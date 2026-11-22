import type { Journey } from "./driver.ts";
import { cardsInSections } from "./journeys/cards-in-sections.ts";
import { outsideChanges } from "./journeys/outside-changes.ts";
import { windowOpens } from "./journeys/window-opens.ts";

export const JOURNEYS: readonly Journey[] = [windowOpens, cardsInSections, outsideChanges];
