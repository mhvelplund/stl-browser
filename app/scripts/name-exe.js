// Copies the release build to target/release/stl-browser-<version>.exe, with the version
// taken from tauri.conf.json (the one Tauri stamps into the exe). Run after `tauri build`.
import { copyFileSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const app = join(dirname(fileURLToPath(import.meta.url)), "..");
const { version } = JSON.parse(readFileSync(join(app, "src-tauri", "tauri.conf.json"), "utf8"));
const release = join(app, "..", "target", "release");
const out = join(release, `stl-browser-${version}.exe`);

copyFileSync(join(release, "app.exe"), out);
console.log(`Copied the release build to ${out}`);
