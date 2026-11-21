import type { BodyChange } from "./BodyChange";
import type { Change } from "./Change";

export type NoteEdit = { title: Change<string> | null, body: BodyChange | null, };
