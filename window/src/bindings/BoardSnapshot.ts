import type { BoardInfo } from "./BoardInfo";
import type { NoteCard } from "./NoteCard";
import type { SectionNode } from "./SectionNode";

export type BoardSnapshot = { board: BoardInfo, sections: Array<SectionNode>, notes: Array<NoteCard>, strays: Array<string>, };
