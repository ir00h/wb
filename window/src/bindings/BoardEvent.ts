import type { NoteCard } from "./NoteCard";
import type { NoteId } from "./NoteId";
import type { SectionNode } from "./SectionNode";
import type { TagHue } from "./TagHue";

export type BoardEvent = { "type": "noteChanged", note: NoteCard, } | { "type": "noteRemoved", note: NoteId, } | { "type": "sectionsChanged", sections: Array<SectionNode>, } | { "type": "tagsChanged", tags: Array<TagHue>, } | { "type": "straysChanged", strays: Array<string>, };
