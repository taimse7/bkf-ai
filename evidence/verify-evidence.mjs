import { createHash } from "node:crypto";
import { createReadStream, existsSync, readFileSync, statSync } from "node:fs";
import { basename, join } from "node:path";
import process from "node:process";

const manifestPath = new URL("./manifest.json", import.meta.url);
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
const roots = process.argv.slice(2);

if (roots.length === 0) {
  console.error("Usage: node evidence/verify-evidence.mjs <sample-directory> [additional-directory]");
  process.exit(2);
}

function locate(fileName) {
  for (const root of roots) {
    const candidate = join(root, fileName);
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

function sha256(path) {
  return new Promise((resolve, reject) => {
    const hash = createHash("sha256");
    const stream = createReadStream(path);
    stream.on("data", (chunk) => hash.update(chunk));
    stream.on("error", reject);
    stream.on("end", () => resolve(hash.digest("hex")));
  });
}

let failures = 0;
for (const sample of manifest.samples) {
  const path = locate(sample.fileName);
  if (!path) {
    console.error(`MISSING ${sample.fileName}`);
    failures += 1;
    continue;
  }

  const size = statSync(path).size;
  const digest = await sha256(path);
  if (size !== sample.size || digest !== sample.sha256) {
    console.error(`MISMATCH ${basename(path)} size=${size} sha256=${digest}`);
    failures += 1;
    continue;
  }
  console.log(`OK ${sample.fileName} size=${size} sha256=${digest}`);
}

if (failures > 0) {
  console.error(`Evidence verification failed: ${failures} item(s)`);
  process.exit(1);
}
console.log(`Evidence verification passed: ${manifest.samples.length} item(s)`);
