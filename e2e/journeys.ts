import type { Journey } from "./driver.ts";
import { cardsInSections } from "./journeys/cards-in-sections.ts";
import { conflictKeepMine, conflictTakeTheirs } from "./journeys/conflict.ts";
import { editOneOperation } from "./journeys/edit-one-operation.ts";
import { look } from "./journeys/look.ts";
import { outsideChanges } from "./journeys/outside-changes.ts";
import { quitKeepsTypedText } from "./journeys/quit.ts";
import { windowOpens } from "./journeys/window-opens.ts";

export const JOURNEYS: readonly Journey[] = [
  windowOpens,
  cardsInSections,
  editOneOperation,
  outsideChanges,
  conflictTakeTheirs,
  conflictKeepMine,
  quitKeepsTypedText,
  look,
];
