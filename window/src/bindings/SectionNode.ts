import type { SectionId } from "./SectionId";

export type SectionNode = { id: SectionId, name: string, note: string, hue: number | null, children: Array<SectionNode>, };
