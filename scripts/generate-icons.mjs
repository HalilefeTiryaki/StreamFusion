// Generates the Tauri app icons (PNG + ICO) without any external tooling.
// Draws a rounded square with a violet→sky gradient and a white play glyph.
import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = join(root, "src-tauri", "icons");
mkdirSync(outDir, { recursive: true });

// --- Minimal PNG encoder -----------------------------------------------------

const crcTable = new Uint32Array(256);
for (let n = 0; n < 256; n++) {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  crcTable[n] = c >>> 0;
}
function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}
function encodePng(size, pixelFn) {
  const stride = size * 4 + 1;
  const raw = Buffer.alloc(size * stride);
  for (let y = 0; y < size; y++) {
    raw[y * stride] = 0; // filter: none
    for (let x = 0; x < size; x++) {
      const [r, g, b, a] = pixelFn(x, y, size);
      const o = y * stride + 1 + x * 4;
      raw[o] = r;
      raw[o + 1] = g;
      raw[o + 2] = b;
      raw[o + 3] = a;
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // color type: RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

// PNG-embedded ICO (valid since Windows Vista).
function icoFromPng(png, size) {
  const header = Buffer.from([0, 0, 1, 0, 1, 0]);
  const entry = Buffer.alloc(16);
  entry[0] = size >= 256 ? 0 : size;
  entry[1] = size >= 256 ? 0 : size;
  entry[4] = 1; // color planes
  entry[6] = 32; // bits per pixel
  entry.writeUInt32LE(png.length, 8);
  entry.writeUInt32LE(22, 12); // data offset
  return Buffer.concat([header, entry, png]);
}

// --- Artwork -----------------------------------------------------------------

const lerp = (a, b, t) => a + (b - a) * t;

// Point-in-triangle test via barycentric signs.
function inTriangle(px, py, ax, ay, bx, by, cx, cy) {
  const s1 = (bx - ax) * (py - ay) - (by - ay) * (px - ax);
  const s2 = (cx - bx) * (py - by) - (cy - by) * (px - bx);
  const s3 = (ax - cx) * (py - cy) - (ay - cy) * (px - cx);
  return (s1 >= 0 && s2 >= 0 && s3 >= 0) || (s1 <= 0 && s2 <= 0 && s3 <= 0);
}

function samplePixel(x, y, size) {
  // 2x2 supersampling for smooth edges.
  let r = 0, g = 0, b = 0, a = 0;
  for (const [dx, dy] of [[0.25, 0.25], [0.75, 0.25], [0.25, 0.75], [0.75, 0.75]]) {
    const u = (x + dx) / size;
    const v = (y + dy) / size;
    const radius = 0.21;

    // Rounded-square mask.
    const cx = Math.max(radius - u, u - (1 - radius), 0);
    const cy = Math.max(radius - v, v - (1 - radius), 0);
    if (Math.sqrt(cx * cx + cy * cy) > radius) continue;

    // Diagonal gradient: violet -> sky.
    const t = (u + v) / 2;
    let sr = lerp(124, 14, t);
    let sg = lerp(58, 165, t);
    let sb = lerp(237, 233, t);

    // White play glyph, slightly left of center.
    if (inTriangle(u, v, 0.36, 0.3, 0.36, 0.7, 0.74, 0.5)) {
      sr = sg = sb = 255;
    }

    r += sr;
    g += sg;
    b += sb;
    a += 255;
  }
  return [Math.round(r / 4), Math.round(g / 4), Math.round(b / 4), Math.round(a / 4)];
}

// --- Emit files ----------------------------------------------------------------

const files = [
  ["32x32.png", 32],
  ["128x128.png", 128],
  ["128x128@2x.png", 256],
  ["icon.png", 512],
];
for (const [name, size] of files) {
  writeFileSync(join(outDir, name), encodePng(size, samplePixel));
  console.log(`wrote icons/${name}`);
}
writeFileSync(join(outDir, "icon.ico"), icoFromPng(encodePng(256, samplePixel), 256));
console.log("wrote icons/icon.ico");
