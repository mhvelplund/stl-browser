<script lang="ts">
  import type { ExtractProgress, IndexProgress, Status } from "./api";

  let {
    status,
    hint,
    progress,
    thumbPending,
    extract,
    onStopIndex,
    onStopExtract,
  }: {
    status: Status | null;
    hint: string;
    progress: IndexProgress | null;
    thumbPending: number;
    extract: ExtractProgress | null;
    onStopIndex: () => void;
    onStopExtract: () => void;
  } = $props();

  const missing = $derived(status?.roots.filter((r) => r.missing) ?? []);
  const plural = (n: number, w: string) => `${n.toLocaleString()} ${w}${n === 1 ? "" : "s"}`;
</script>

<footer>
  {#if status}
    <span class="mono">
      {plural(status.roots.length, "folder")} indexed · {plural(status.models, "model")} · {plural(status.files, "file")}
    </span>
  {/if}
  {#if missing.length}
    <span class="warn" title={missing.map((r) => r.path).join("\n")}>
      {plural(missing.length, "folder")} not found
    </span>
  {/if}
  <span class="hint" title={hint}>{hint}</span>
  {#if progress}
    <div class="task" role="status">
      <span>Indexing {progress.index} / {progress.total}</span>
      <div class="bar"><div style:width="{(100 * progress.index) / Math.max(1, progress.total)}%"></div></div>
      <span class="source" title={progress.name}>{progress.name}</span>
      <button type="button" class="stop" aria-label="Stop indexing" title="Stop indexing" onclick={onStopIndex}>
        <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="M3 3l6 6M9 3l-6 6" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg>
      </button>
    </div>
  {/if}
  {#if extract}
    <div class="task">
      <span>Extracting {extract.index} / {extract.total}</span>
      <div class="bar"><div style:width="{(100 * extract.index) / Math.max(1, extract.total)}%"></div></div>
      <span class="source" title={extract.name}>{extract.name}</span>
      <button type="button" class="stop" aria-label="Stop extracting" title="Stop extracting" onclick={onStopExtract}>
        <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="M3 3l6 6M9 3l-6 6" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg>
      </button>
    </div>
  {/if}
  <span class="mono">{thumbPending > 0 ? `Thumbnails: ${thumbPending.toLocaleString()} queued` : "Thumbnails up to date"}</span>
</footer>

<style>
  footer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 24px;
    padding: 10px 20px;
    background: var(--bar);
    border-top: 1px solid var(--line);
    font-size: 12px;
    color: var(--muted);
  }
  .mono {
    font-family: var(--mono);
  }
  .warn {
    color: var(--warn);
  }
  .hint {
    flex: 1 1 280px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
  }
  .task {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .bar {
    width: 160px;
    height: 6px;
    border-radius: 3px;
    background: var(--line);
    overflow: hidden;
  }
  .bar > div {
    height: 100%;
    background: var(--accent);
  }
  .stop {
    display: inline-grid;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }
  .stop:hover {
    color: var(--error);
    border-color: var(--error);
  }
  .source {
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--mono);
  }
</style>
