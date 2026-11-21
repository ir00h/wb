import type { SectionRef } from "./SectionRef";

export type DraftSection = { "type": "default" } | { "type": "noSection" } | { "type": "named", section: SectionRef, };
