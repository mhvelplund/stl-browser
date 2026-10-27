<script lang="ts">
  import type { IndexProgress, Status } from "./api";

  let { status, hint, progress }: { status: Status | null; hint: string; progress: IndexProgress | null } = $props();

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
    </div>
  {/if}
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
  .source {
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--mono);
  }
</style>
