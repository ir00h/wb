import { readFile } from "node:fs/promises";
import { inflateSync } from "node:zlib";

const SIGNATURE = Uint8Array.of(0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a);
const BYTES_PER_PIXEL: Readonly<Record<number, number>> = { 2: 3, 6: 4 };

export type Png = {
  readonly width: number;
  readonly height: number;
  readonly bytes: number;
  readonly colours: number;
};

type Header = {
  width: number;
  height: number;
  colourType: number;
  bitDepth: number;
  interlace: number;
};

function chunks(file: Buffer): { header: Header; data: Buffer } {
  let header: Header | null = null;
  const data: Buffer[] = [];
  let offset = SIGNATURE.length;
  while (offset + 8 <= file.length) {
    const length = file.readUInt32BE(offset);
    const type = file.toString("latin1", offset + 4, offset + 8);
    const body = file.subarray(offset + 8, offset + 8 + length);
    if (type === "IHDR") {
      header = {
        width: body.readUInt32BE(0),
        height: body.readUInt32BE(4),
        bitDepth: body.readUInt8(8),
        colourType: body.readUInt8(9),
        interlace: body.readUInt8(12),
      };
    } else if (type === "IDAT") {
      data.push(body);
    }
    offset += 12 + length;
  }
  if (header === null) {
    throw new Error("the PNG file has no header chunk");
  }
  return { header, data: Buffer.concat(data) };
}

function paeth(left: number, up: number, upLeft: number): number {
  const estimate = left + up - upLeft;
  const toLeft = Math.abs(estimate - left);
  const toUp = Math.abs(estimate - up);
  const toUpLeft = Math.abs(estimate - upLeft);
  if (toLeft <= toUp && toLeft <= toUpLeft) {
    return left;
  }
  return toUp <= toUpLeft ? up : upLeft;
}

type Predictor = (left: number, up: number, upLeft: number) => number;

const PREDICTORS: readonly Predictor[] = [
  () => 0,
  (left) => left,
  (_left, up) => up,
  (left, up) => (left + up) >> 1,
  paeth,
];

function unfilter(data: Buffer, header: Header, pixelBytes: number): Uint8Array {
  const rowBytes = header.width * pixelBytes;
  const pixels = new Uint8Array(rowBytes * header.height);
  for (let row = 0; row < header.height; row += 1) {
    const source = row * (rowBytes + 1);
    const target = row * rowBytes;
    const filter = data[source] ?? -1;
    const predict = PREDICTORS[filter];
    if (predict === undefined) {
      throw new Error(`row ${row} of the PNG file has an unknown filter ${filter}`);
    }
    for (let index = 0; index < rowBytes; index += 1) {
      const hasLeft = index >= pixelBytes;
      const left = hasLeft ? (pixels[target + index - pixelBytes] ?? 0) : 0;
      const up = row > 0 ? (pixels[target - rowBytes + index] ?? 0) : 0;
      const upLeft = row > 0 && hasLeft ? (pixels[target - rowBytes + index - pixelBytes] ?? 0) : 0;
      pixels[target + index] = ((data[source + 1 + index] ?? 0) + predict(left, up, upLeft)) & 0xff;
    }
  }
  return pixels;
}

function countColours(pixels: Uint8Array, pixelBytes: number, atMost: number): number {
  const seen = new Set<string>();
  for (let offset = 0; offset < pixels.length && seen.size < atMost; offset += pixelBytes) {
    seen.add(pixels.subarray(offset, offset + pixelBytes).join(","));
  }
  return seen.size;
}

export async function readPng(path: string, countColoursUpTo = 16): Promise<Png> {
  const file = await readFile(path);
  if (!SIGNATURE.every((byte, index) => file[index] === byte)) {
    throw new Error(`${path} is not a PNG file`);
  }
  const { header, data } = chunks(file);
  const pixelBytes = BYTES_PER_PIXEL[header.colourType];
  if (pixelBytes === undefined || header.bitDepth !== 8 || header.interlace !== 0) {
    throw new Error(
      `${path} is a PNG this reader does not decode (colour type ${header.colourType}, bit depth ${header.bitDepth}, interlace ${header.interlace})`,
    );
  }
  const pixels = unfilter(inflateSync(data), header, pixelBytes);
  return {
    width: header.width,
    height: header.height,
    bytes: file.length,
    colours: countColours(pixels, pixelBytes, countColoursUpTo),
  };
}
