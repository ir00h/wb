import type { NoteCard } from "../bindings/NoteCard";
import type { Tag } from "../bindings/Tag";
import type { TagHue } from "../bindings/TagHue";

export type CardLook =
  | { readonly kind: "plain" }
  | { readonly kind: "tinted"; readonly tag: Tag; readonly hue: number };

export function cardLook(card: NoteCard, hues: readonly TagHue[]): CardLook {
  for (const tag of card.tags) {
    const coloured = hues.find((entry) => entry.tag === tag);
    if (coloured !== undefined) {
      return { kind: "tinted", tag, hue: coloured.hue };
    }
  }
  return { kind: "plain" };
}
