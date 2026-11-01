// Release build of the app alone, with this machine's paths remapped so the binary doesn't
// embed them: Rust keeps source paths for panic locations, and dependency sources live under
// the user's home (~/.cargo/registry/...). Then names the binary. With --installer, builds the
// installer instead, which Tauri names itself: NSIS on Windows (target/release/bundle/nsis/),
// a DMG on macOS. macOS builds are universal (Apple Silicon + Intel), so they land in
// target/universal-apple-darwin/release/ and need both Apple Rust targets installed.
import { spawnSync } from "node:child_process";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const app = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repo = resolve(app, "..");
const cargoHome = process.env.CARGO_HOME || join(homedir(), ".cargo");
const remaps = [`--remap-path-prefix=${cargoHome}=/cargo`, `--remap-path-prefix=${repo}=/src`];
const rustflags = [process.env.RUSTFLAGS, ...remaps].filter(Boolean).join(" ");

const run = (cmd, args, env) => {
  const r = spawnSync(cmd, args, { cwd: app, stdio: "inherit", shell: process.platform === "win32", env });
  if (r.status !== 0) process.exit(r.status ?? 1);
};

const mac = process.platform === "darwin";
const installer = process.argv.includes("--installer");
const target = mac ? ["--target", "universal-apple-darwin"] : [];
const bundle = installer ? ["--bundles", mac ? "dmg" : "nsis"] : ["--no-bundle"];
run("npx", ["tauri", "build", ...target, ...bundle], { ...process.env, RUSTFLAGS: rustflags });
if (!installer) run("node", [join(app, "scripts", "name-exe.js")], process.env);
