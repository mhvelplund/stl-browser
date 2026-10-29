// The index run as the UI sees it. `index-done` is emitted by the worker thread and can arrive
// before the reply to the command that started the run, so only `start`, `refused` and `done`
// change the hint; `accepted` (the reply) never overwrites a result.
import type { IndexDone } from "./api.ts";

export type Run = { busy: boolean; hint: string };

export type RunEvent =
  | { type: "start"; hint: string }
  | { type: "accepted" }
  | { type: "refused"; error: string }
  | { type: "done"; done: IndexDone };

export const idle: Run = { busy: false, hint: "" };

const n = (count: number, word: string) => `${count} ${word}${count === 1 ? "" : "s"}`;

function doneHint(d: IndexDone): string {
  if (!d.ok) return `Index failed: ${d.error}`;
  if (d.cancelled) {
    return `Stopped: ${d.listed} listed · ${n(d.errors, "error")} · ${n(d.models, "model")} (${d.seconds.toFixed(1)} s)`;
  }
  const warn = d.warnings.length ? ` · ${n(d.warnings.length, "warning")}: ${d.warnings.join("; ")}` : "";
  return (
    `Indexed ${n(d.sources, "source")} · ${d.listed} listed · ${d.removed} removed · ${n(d.errors, "error")} · ` +
    `${n(d.models, "model")} (${d.seconds.toFixed(1)} s)${warn}`
  );
}

export function step(run: Run, e: RunEvent): Run {
  switch (e.type) {
    case "start":
      return { busy: true, hint: e.hint };
    case "accepted":
      return run;
    case "refused":
      return { busy: false, hint: `Could not start indexing: ${e.error}` };
    case "done":
      return { busy: false, hint: doneHint(e.done) };
  }
}
