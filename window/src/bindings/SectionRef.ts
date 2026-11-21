import type { SectionId } from "./SectionId";
import type { SectionPath } from "./SectionPath";

export type SectionRef = { "type": "id", id: SectionId, } | { "type": "path", path: SectionPath, };
