// The status-bar line for a finished Extract to….
import type { ExtractDone } from "./api.ts";

const n = (count: number, word: string) => `${count} ${word}${count === 1 ? "" : "s"}`;

export function extractHint(d: ExtractDone): string {
  if (d.error) return `Extract failed: ${d.error}`;
  let hint = `${d.cancelled ? "Stopped after" : "Extracted"} ${n(d.files, "file")} to ${d.dest} (${d.seconds.toFixed(1)} s)`;
  if (d.skipped) hint += ` · ${d.skipped} locked skipped`;
  if (d.failed.length) hint += ` · ${d.failed.length} failed: ${d.failed.map(([rel, e]) => `${rel} (${e})`).join("; ")}`;
  return hint;
}
