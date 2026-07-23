import { execFileSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, rmSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve("plugins/otzaria-bkf-bkc");
const dist = resolve(root, "dist");
if (!existsSync(dist)) {
  throw new Error("Plugin dist is missing. Run pnpm plugin:build first.");
}

const staging = resolve(root, ".package");
const output = resolve(root, "otzaria-bkf-bkc.otzplugin");
rmSync(staging, { recursive: true, force: true });
rmSync(output, { force: true });
mkdirSync(staging, { recursive: true });

cpSync(dist, staging, { recursive: true });
cpSync(resolve(root, "manifest.json"), resolve(staging, "manifest.json"));

execFileSync("zip", ["-qr", output, "."], { cwd: staging, stdio: "inherit" });
rmSync(staging, { recursive: true, force: true });
console.log(output);
