import { readFileSync, existsSync } from "node:fs";

const required = [
  "package.json",
  "src/App.tsx",
  "src-tauri/Cargo.toml",
  "src-tauri/src/lib.rs",
  "src-tauri/catalog/src/lib.rs",
  "src-tauri/scanner-core/src/lib.rs",
  "src-tauri/converter-core/src/lib.rs",
  "src-tauri/bkf-core/src/lib.rs",
  "src-tauri/search-core/src/lib.rs",
  "src-tauri/local-api/src/lib.rs",
  "plugins/otzaria-bkf-bkc/manifest.json",
  "ARCHITECTURE.md",
  "KNOWN_LIMITATIONS.md"
];

const missing = required.filter((path) => !existsSync(path));
if (missing.length > 0) {
  console.error("Missing required files:");
  missing.forEach((path) => console.error(`- ${path}`));
  process.exit(1);
}

for (const path of [
  "package.json",
  "src-tauri/tauri.conf.json",
  "src-tauri/capabilities/default.json",
  "plugins/otzaria-bkf-bkc/package.json",
  "plugins/otzaria-bkf-bkc/manifest.json"
]) {
  JSON.parse(readFileSync(path, "utf8"));
}

console.log(`Project structure verified (${required.length} required files).`);
