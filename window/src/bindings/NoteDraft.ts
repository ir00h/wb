import type { SectionRef } from "./SectionRef";

export type NoteDraft = { title: string, body: string, section: SectionRef | null, tags: Array<string>, };
