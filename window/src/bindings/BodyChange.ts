import type { Change } from "./Change";

export type BodyChange = { "type": "set" } & Change<string> | { "type": "append", text: string, };
