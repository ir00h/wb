import type { DraftSection } from "./DraftSection";

export type NoteDraft = { title: string, body: string, section: DraftSection, tags: Array<string>, source: string, };
