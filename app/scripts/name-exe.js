// Copies the release build to a versioned name, with the version taken from tauri.conf.json (the
// one Tauri stamps into the binary). Run after `tauri build`. Windows: target/release/app.exe ->
// stl-browser-<version>.exe. macOS: the universal binary target/universal-apple-darwin/release/app
// -> stl-browser-<version>-macos next to it.
import { chmodSync, copyFileSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const app = join(dirname(fileURLToPath(import.meta.url)), "..");
const { version } = JSON.parse(readFileSync(join(app, "src-tauri", "tauri.conf.json"), "utf8"));
const mac = process.platform === "darwin";
const release = mac ? join(app, "..", "target", "universal-apple-darwin", "release") : join(app, "..", "target", "release");
const src = join(release, mac ? "app" : "app.exe");
const out = join(release, mac ? `stl-browser-${version}-macos` : `stl-browser-${version}.exe`);

try {
  copyFileSync(src, out);
} catch (e) {
  // Windows locks a running exe, so a copy of the app that is still open can't be replaced
  if (e.code === "EBUSY" || e.code === "EPERM") {
    console.error(`Can't overwrite ${out}: it is probably still running. Close it and run this again.`);
    process.exit(1);
  }
  throw e;
}
if (mac) chmodSync(out, 0o755);
console.log(`Copied the release build to ${out}`);
