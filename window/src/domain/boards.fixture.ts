import type { BoardId } from "../bindings/BoardId";
import type { BoardSnapshot } from "../bindings/BoardSnapshot";
import type { NoteCard } from "../bindings/NoteCard";
import type { NoteId } from "../bindings/NoteId";
import type { SectionId } from "../bindings/SectionId";
import type { SectionNode } from "../bindings/SectionNode";
import type { Tag } from "../bindings/Tag";
import type { TagHue } from "../bindings/TagHue";

export const noteId = (text: string) => text as NoteId;
export const sectionId = (text: string) => text as SectionId;
export const tag = (text: string) => text as Tag;

export function note(
  id: string,
  title: string,
  section: string | null,
  tags: readonly string[] = [],
): NoteCard {
  return {
    id: noteId(id),
    title,
    body: "",
    tags: tags.map(tag),
    section: section === null ? null : sectionId(section),
  };
}

export function section(
  id: string,
  name: string,
  children: readonly SectionNode[] = [],
): SectionNode {
  return { id: sectionId(id), name, note: "", hue: null, children: [...children] };
}

export function board(
  sections: readonly SectionNode[],
  notes: readonly NoteCard[],
  tags: readonly TagHue[] = [],
): BoardSnapshot {
  return {
    board: { id: "garden" as BoardId, name: "Garden" },
    sections: [...sections],
    tags: [...tags],
    notes: [...notes],
    strays: [],
  };
}
