import type { NoteCard } from "./NoteCard";
import type { SectionNode } from "./SectionNode";

export type Outcome = { "type": "applied", note: NoteCard, } | { "type": "conflict", current: NoteCard, } | { "type": "sectionAdded", section: SectionNode, };
