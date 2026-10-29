// Release build of the exe alone (no installers), with this machine's paths remapped so the
// binary doesn't embed them: Rust keeps source paths for panic locations, and dependency
// sources live under the user's home (~/.cargo/registry/...). Then names the exe.
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

run("npx", ["tauri", "build", "--no-bundle"], { ...process.env, RUSTFLAGS: rustflags });
run("node", [join(app, "scripts", "name-exe.js")], process.env);
