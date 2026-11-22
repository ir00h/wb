import type { BoardInfo } from "./BoardInfo";
import type { NoteCard } from "./NoteCard";
import type { SectionNode } from "./SectionNode";
import type { TagHue } from "./TagHue";

export type BoardSnapshot = { board: BoardInfo, sections: Array<SectionNode>, tags: Array<TagHue>, notes: Array<NoteCard>, strays: Array<string>, };
