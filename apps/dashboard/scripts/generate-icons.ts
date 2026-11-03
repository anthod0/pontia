import { mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import sharp from "sharp";

const publicDir = resolve(import.meta.dirname, "../public");
const sourcePath = resolve(publicDir, "logo.svg");
const pngSizes = [32, 180, 192, 512] as const;
const icoSizes = [16, 32, 48, 64] as const;
// iOS fills transparent apple-touch-icon pixels with black, so that icon is
// flattened onto the dashboard background.
const opaqueSizes = new Map<number, string>([[180, "#fafaf8"]]);

function createIco(images: ReadonlyArray<{ size: number; data: Buffer }>): Buffer {
  const directorySize = 6 + images.length * 16;
  const header = Buffer.alloc(directorySize);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(images.length, 4);

  let imageOffset = directorySize;
  for (const [index, image] of images.entries()) {
    const entryOffset = 6 + index * 16;
    header.writeUInt8(image.size === 256 ? 0 : image.size, entryOffset);
    header.writeUInt8(image.size === 256 ? 0 : image.size, entryOffset + 1);
    header.writeUInt8(0, entryOffset + 2);
    header.writeUInt8(0, entryOffset + 3);
    header.writeUInt16LE(1, entryOffset + 4);
    header.writeUInt16LE(32, entryOffset + 6);
    header.writeUInt32LE(image.data.length, entryOffset + 8);
    header.writeUInt32LE(imageOffset, entryOffset + 12);
    imageOffset += image.data.length;
  }

  return Buffer.concat([header, ...images.map(({ data }) => data)]);
}

const startedAt = performance.now();
const source = await readFile(sourcePath);
const { width: sourceSize } = await sharp(source).metadata();
const sizes = [...new Set([...pngSizes, ...icoSizes])];

async function render(size: number): Promise<Buffer> {
  // Rasterize the SVG directly at the target size instead of resampling.
  let image = sharp(source, { density: (72 * size) / sourceSize }).resize(size, size);
  const background = opaqueSizes.get(size);
  if (background) image = image.flatten({ background });
  return image.png().toBuffer();
}

const rendered = new Map(
  await Promise.all(sizes.map(async (size) => [size, await render(size)] as const)),
);

await mkdir(publicDir, { recursive: true });
await Promise.all([
  ...pngSizes.map((size) => writeFile(resolve(publicDir, `logo-${size}.png`), rendered.get(size)!)),
  writeFile(
    resolve(publicDir, "favicon.ico"),
    createIco(icoSizes.map((size) => ({ size, data: rendered.get(size)! }))),
  ),
]);

console.log(`Generated Dashboard icons in ${(performance.now() - startedAt).toFixed(0)} ms`);
