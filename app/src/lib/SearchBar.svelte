<script lang="ts">
  import { onMount } from "svelte";
  import type { SearchMode } from "./api";
  import { scopeLabel } from "./search";

  let {
    query = $bindable(),
    mode = $bindable(),
    caseSensitive = $bindable(),
    showCase,
    scope,
    error,
    note,
    onClose,
  }: {
    query: string;
    mode: SearchMode;
    caseSensitive: boolean;
    /** Only the Files view keeps the case of names; the Models view's are lowercase. */
    showCase: boolean;
    scope: string[];
    error: string;
    note: string;
    onClose: () => void;
  } = $props();

  const modes: { id: SearchMode; label: string; title: string }[] = [
    { id: "free", label: "Free", title: "The letters appear in order, not necessarily together" },
    { id: "word", label: "Word", title: "The text appears as one piece" },
    { id: "regexp", label: "Regexp", title: "The name matches a regular expression" },
  ];

  let input: HTMLInputElement | undefined = $state();
  onMount(() => input?.focus());

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }
</script>

<div class="bar">
  <input bind:this={input} bind:value={query} type="search" placeholder="Search names" aria-label="Search names" spellcheck="false" onkeydown={key} />
  <div class="seg-group" role="group" aria-label="Match">
    {#each modes as m (m.id)}
      <button type="button" class="seg" class:on={mode === m.id} aria-pressed={mode === m.id} title={m.title} onclick={() => (mode = m.id)}>{m.label}</button>
    {/each}
  </div>
  {#if showCase}
    <button type="button" class="case" class:on={caseSensitive} aria-pressed={caseSensitive} aria-label="Match case" title="Match case" onclick={() => (caseSensitive = !caseSensitive)}>Aa</button>
  {/if}
  <span class="scope" title={scope.join("/")}>{scopeLabel(scope)}</span>
  {#if error}
    <span class="error">{error}</span>
  {:else if note}
    <span class="note">{note}</span>
  {/if}
</div>

<style>
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    padding: 10px 20px;
    background: var(--bar);
    border-bottom: 1px solid var(--line);
  }
  input {
    flex: 1 1 280px;
    min-width: 0;
    height: 34px;
    padding: 0 12px;
    border-radius: 8px;
    border: 1px solid var(--ctl-line);
    background: var(--ctl);
    color: var(--text);
    font: 13px var(--font);
  }
  input:focus {
    outline: none;
    border-color: var(--sel-line);
  }
  .seg-group {
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: 8px;
    background: var(--ctl);
    border: 1px solid var(--ctl-line);
  }
  .seg,
  .case {
    height: 28px;
    padding: 0 12px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--muted);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
  }
  .case {
    height: 34px;
    border: 1px solid var(--ctl-line);
    border-radius: 8px;
    background: var(--ctl);
  }
  .seg.on,
  .case.on {
    background: var(--ctl-line);
    color: var(--text-strong);
  }
  .scope,
  .note {
    color: var(--muted);
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 40ch;
  }
  .error {
    color: var(--error);
    font-size: 13px;
  }
</style>
