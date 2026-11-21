import type { NoteId } from "./NoteId";
import type { SectionId } from "./SectionId";
import type { Tag } from "./Tag";

export type NoteCard = { id: NoteId, title: string, body: string, tags: Array<Tag>, section: SectionId | null, };
