import type { BoardId } from "./BoardId";
import type { Invalid } from "./Invalid";
import type { NoteId } from "./NoteId";
import type { SectionRef } from "./SectionRef";

export type Error = { "type": "workspaceNotFound", path: string, } | { "type": "boardNotFound", board: BoardId, } | { "type": "noteNotFound", note: NoteId, } | { "type": "duplicateNote", note: NoteId, files: Array<string>, } | { "type": "sectionNotFound", section: SectionRef, } | { "type": "invalid", problem: Invalid, } | { "type": "busy", what: string, waitedMs: number, } | { "type": "locksUnsupported", path: string, } | { "type": "malformed", path: string, message: string, } | { "type": "io", path: string, message: string, };
