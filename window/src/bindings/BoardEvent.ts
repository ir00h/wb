import type { NoteCard } from "./NoteCard";
import type { NoteId } from "./NoteId";
import type { SectionNode } from "./SectionNode";

export type BoardEvent = { "type": "noteChanged", note: NoteCard, } | { "type": "noteRemoved", note: NoteId, } | { "type": "sectionsChanged", sections: Array<SectionNode>, };
