import type { BoardSnapshot } from "../bindings/BoardSnapshot";
import type { NoteCard } from "../bindings/NoteCard";
import type { NoteId } from "../bindings/NoteId";
import type { SectionId } from "../bindings/SectionId";
import type { SectionNode } from "../bindings/SectionNode";

export const CARD_WIDTH = 236;
export const ESTIMATED_CARD_HEIGHT = 96;
export const FRAME_PADDING = 20;
export const COLUMN_GAP = 20;
export const CARD_GAP = 12;
export const FRAME_GAP = 48;
export const COLUMN_LIMIT = 1200;
export const FRAME_MIN_HEIGHT = 120;
export const BOARD_ORIGIN = { x: 40, y: 44 } as const;

export type Rect = {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
};

export type FramePlacement = {
  readonly section: SectionNode;
  readonly depth: number;
  readonly rect: Rect;
};

export type CardPlacement = {
  readonly note: NoteId;
  readonly section: SectionId | null;
  readonly rect: Rect;
};

export type Layout = {
  readonly frames: readonly FramePlacement[];
  readonly cards: readonly CardPlacement[];
  readonly settled: boolean;
};

export type Heights = ReadonlyMap<NoteId, number>;

type Placed = { readonly frames: FramePlacement[]; readonly cards: CardPlacement[] };

type Columns = { readonly right: number; readonly bottom: number };

function heightOf(note: NoteId, heights: Heights): number {
  return heights.get(note) ?? ESTIMATED_CARD_HEIGHT;
}

function flowColumns(
  notes: readonly NoteCard[],
  section: SectionId | null,
  origin: { readonly x: number; readonly y: number },
  heights: Heights,
  placed: Placed,
): Columns {
  if (notes.length === 0) {
    return { right: origin.x, bottom: origin.y };
  }
  let x = origin.x;
  let y = origin.y;
  let bottom = origin.y;
  for (const note of notes) {
    const height = heightOf(note.id, heights);
    if (y > origin.y && y + height - origin.y > COLUMN_LIMIT) {
      x += CARD_WIDTH + COLUMN_GAP;
      y = origin.y;
    }
    placed.cards.push({ note: note.id, section, rect: { x, y, width: CARD_WIDTH, height } });
    y += height + CARD_GAP;
    bottom = Math.max(bottom, y - CARD_GAP);
  }
  return { right: x + CARD_WIDTH, bottom };
}

function placeFrame(
  section: SectionNode,
  depth: number,
  origin: { readonly x: number; readonly y: number },
  notesOf: ReadonlyMap<SectionId, readonly NoteCard[]>,
  heights: Heights,
  placed: Placed,
): Rect {
  const index = placed.frames.length;
  const inside = { x: origin.x + FRAME_PADDING, y: origin.y + FRAME_PADDING };
  const columns = flowColumns(notesOf.get(section.id) ?? [], section.id, inside, heights, placed);
  let right = columns.right;
  let bottom = columns.bottom;
  let childX = columns.right === inside.x ? inside.x : columns.right + COLUMN_GAP;
  for (const child of section.children) {
    const rect = placeFrame(child, depth + 1, { x: childX, y: origin.y }, notesOf, heights, placed);
    right = rect.x + rect.width;
    bottom = Math.max(bottom, rect.y + rect.height);
    childX = right + COLUMN_GAP;
  }
  const rect = {
    x: origin.x,
    y: origin.y,
    width: Math.max(right + FRAME_PADDING, inside.x + CARD_WIDTH + FRAME_PADDING) - origin.x,
    height: Math.max(bottom + FRAME_PADDING - origin.y, FRAME_MIN_HEIGHT),
  };
  placed.frames.splice(index, 0, { section, depth, rect });
  return rect;
}

function sectionIds(sections: readonly SectionNode[]): Set<SectionId> {
  const ids = new Set<SectionId>();
  const visit = (nodes: readonly SectionNode[]) => {
    for (const node of nodes) {
      ids.add(node.id);
      visit(node.children);
    }
  };
  visit(sections);
  return ids;
}

export function layout(snapshot: BoardSnapshot, heights: Heights): Layout {
  const known = sectionIds(snapshot.sections);
  const notesOf = new Map<SectionId, NoteCard[]>();
  const loose: NoteCard[] = [];
  for (const note of snapshot.notes) {
    if (note.section !== null && known.has(note.section)) {
      const list = notesOf.get(note.section) ?? [];
      list.push(note);
      notesOf.set(note.section, list);
    } else {
      loose.push(note);
    }
  }
  const placed: Placed = { frames: [], cards: [] };
  let x = BOARD_ORIGIN.x;
  for (const section of snapshot.sections) {
    const rect = placeFrame(section, 0, { x, y: BOARD_ORIGIN.y }, notesOf, heights, placed);
    x = rect.x + rect.width + FRAME_GAP;
  }
  flowColumns(loose, null, { x, y: BOARD_ORIGIN.y }, heights, placed);
  return { ...placed, settled: placed.cards.every((card) => heights.has(card.note)) };
}

export function contains(outer: Rect, inner: Rect): boolean {
  return (
    inner.x >= outer.x &&
    inner.y >= outer.y &&
    inner.x + inner.width <= outer.x + outer.width &&
    inner.y + inner.height <= outer.y + outer.height
  );
}
