import { readFile } from "node:fs/promises";

const SIGNATURE = Uint8Array.of(0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a);
const WIDTH_OFFSET = 16;
const HEIGHT_OFFSET = 20;

export type PngSize = { readonly width: number; readonly height: number; readonly bytes: number };

export async function pngSize(path: string): Promise<PngSize> {
  const file = await readFile(path);
  if (file.length <= HEIGHT_OFFSET + 4 || !SIGNATURE.every((byte, index) => file[index] === byte)) {
    throw new Error(`${path} is not a PNG file`);
  }
  return {
    width: file.readUInt32BE(WIDTH_OFFSET),
    height: file.readUInt32BE(HEIGHT_OFFSET),
    bytes: file.length,
  };
}
