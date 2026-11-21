import type { NoteDraft } from "./NoteDraft";
import type { NoteEdit } from "./NoteEdit";
import type { NoteId } from "./NoteId";
import type { SectionRef } from "./SectionRef";

export type Op = { "type": "addSection", name: string, parent: SectionRef | null, } | { "type": "addNote", draft: NoteDraft, } | { "type": "editNote", note: NoteId, edit: NoteEdit, };
