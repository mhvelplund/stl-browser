// Copies the release build to target/release/stl-browser-<version>.exe, with the version
// taken from tauri.conf.json (the one Tauri stamps into the exe). Run after `tauri build`.
import { copyFileSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const app = join(dirname(fileURLToPath(import.meta.url)), "..");
const { version } = JSON.parse(readFileSync(join(app, "src-tauri", "tauri.conf.json"), "utf8"));
const release = join(app, "..", "target", "release");
const out = join(release, `stl-browser-${version}.exe`);

try {
  copyFileSync(join(release, "app.exe"), out);
} catch (e) {
  // Windows locks a running exe, so a copy of the app that is still open can't be replaced
  if (e.code === "EBUSY" || e.code === "EPERM") {
    console.error(`Can't overwrite ${out}: it is probably still running. Close it and run this again.`);
    process.exit(1);
  }
  throw e;
}
console.log(`Copied the release build to ${out}`);
