import type { Journey } from "./driver.ts";
import { windowOpens } from "./journeys/window-opens.ts";

export const JOURNEYS: readonly Journey[] = [windowOpens];
