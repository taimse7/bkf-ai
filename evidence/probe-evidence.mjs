import { open, readFile, writeFile } from "node:fs/promises";
import { basename, resolve } from "node:path";

const TAIL_WINDOW = 2 * 1024 * 1024;
const HEAD_WINDOW = 64 * 1024;
const root = resolve(process.argv[2] ?? "../upload");
const output = resolve(process.argv[3] ?? "evidence/probe-results.json");
const names = ["157.book", "221.book", "232.book", "241.book", "243.book", "688840.book"];

function lastIndexOfBytes(buffer, text, before = buffer.length) {
  return buffer.subarray(0, before).lastIndexOf(Buffer.from(text));
}

function parseBkc(tail, tailStart) {
  const marker = lastIndexOfBytes(tail, "startxref");
  if (marker < 0) throw new Error("last startxref was not found");
  const value = tail.subarray(marker + 9).toString("latin1").match(/^\s*(\d+)/);
  if (!value) throw new Error("last startxref value is invalid");
  const startxref = Number(value[1]);
  const eofRelative = tail.indexOf(Buffer.from("%%EOF"), marker);
  if (eofRelative < 0) throw new Error("matching EOF marker was not found");

  const before = tail.subarray(0, marker).toString("latin1");
  const matches = [...before.matchAll(/\/Type\s*\/XRef/g)];
  if (!matches.length) throw new Error("physical XRef stream was not found");
  const typeAt = matches.at(-1).index;
  const headers = [...before.slice(0, typeAt).matchAll(/(?:^|[\r\n])(\d+)\s+(\d+)\s+obj/g)];
  const header = headers.at(-1);
  if (!header) throw new Error("XRef object header was not found");
  const leadingBreak = /^[\r\n]/.test(header[0]) ? 1 : 0;
  const relativeHeader = header.index + leadingBreak;
  const physicalXref = tailStart + relativeHeader;
  const baseOffset = physicalXref - startxref;
  if (!Number.isSafeInteger(baseOffset) || baseOffset < 0) {
    throw new Error("computed base offset is invalid");
  }
  return {
    startxref,
    physicalXref,
    baseOffset,
    xrefObjectNumber: Number(header[1]),
    eofPhysicalOffset: tailStart + eofRelative,
  };
}

async function probe(path) {
  const handle = await open(path, "r");
  try {
    const { size } = await handle.stat();
    const head = Buffer.alloc(Math.min(HEAD_WINDOW, size));
    await handle.read(head, 0, head.length, 0);
    const magic = head.subarray(0, 3).toString("ascii");
    if (magic === "BKF") {
      return {
        file: basename(path), kind: "bkf", fileSize: size, decoderAvailable: false,
        bkf: {
          standardDjvuSignatureVisible: head.includes("AT&TFORM") || head.includes("DJVU"),
          pageIndexStatus: "unknown",
        },
      };
    }
    if (magic !== "BKC") {
      return { file: basename(path), kind: "unknown", fileSize: size, decoderAvailable: false };
    }
    const tailStart = Math.max(0, size - TAIL_WINDOW);
    const tail = Buffer.alloc(size - tailStart);
    await handle.read(tail, 0, tail.length, tailStart);
    return {
      file: basename(path), kind: "bkc", fileSize: size, decoderAvailable: false,
      bkc: parseBkc(tail, tailStart),
    };
  } finally {
    await handle.close();
  }
}

const results = [];
for (const name of names) results.push(await probe(resolve(root, name)));
await writeFile(output, `${JSON.stringify({ formatVersion: 1, results }, null, 2)}\n`);
for (const result of results) {
  const structural = result.bkc ? ` baseOffset=${result.bkc.baseOffset}` : "";
  console.log(`${result.file}: ${result.kind}${structural}; decoder=${result.decoderAvailable}`);
}
console.log(`Saved ${output}`);
