<script lang="ts">
  import { onMount, tick } from "svelte";
  import * as api from "./api";
  import type { SearchMode, Tag } from "./api";
  import { scopeLabel } from "./search";
  import { applyCompletion, matchTags, tagTermAt } from "./tagcomplete";

  let {
    query = $bindable(),
    mode = $bindable(),
    caseSensitive = $bindable(),
    asList = $bindable(),
    showCase,
    scope,
    error,
    note,
    onClose,
  }: {
    query: string;
    mode: SearchMode;
    caseSensitive: boolean;
    /** Show the results as a compact list instead of tiles. */
    asList: boolean;
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
  let list: HTMLUListElement | undefined = $state();
  let known = $state<Tag[]>([]);
  let caret = $state(0);
  let active = $state(0);
  /** Esc closed the list; it opens again on the next edit or caret move. */
  let dismissed = $state(false);

  const term = $derived(tagTermAt(query, caret));
  const matches = $derived(term ? matchTags(known, term.prefix) : []);
  const open = $derived(!dismissed && matches.length > 0);

  async function loadKnown() {
    try {
      known = await api.allTags();
    } catch {
      // no suggestions; the search itself still works
    }
  }

  onMount(() => {
    input?.focus();
    loadKnown();
  });

  $effect(() => {
    // the first match is selected whenever the list changes
    void matches;
    active = 0;
  });

  function track() {
    const c = input?.selectionStart ?? query.length;
    if (c !== caret) dismissed = false;
    caret = c;
  }

  function onInput() {
    dismissed = false;
    track();
  }

  async function choose(name: string) {
    if (!term) return;
    const done = applyCompletion(query, term, name);
    query = done.query;
    caret = done.caret;
    await tick();
    input?.setSelectionRange(done.caret, done.caret);
  }

  async function move(by: number) {
    active = (active + by + matches.length) % matches.length;
    await tick();
    list?.querySelector(`[data-i="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }

  function key(e: KeyboardEvent) {
    if (open) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        move(e.key === "ArrowDown" ? 1 : -1);
        return;
      }
      if (e.key === "Tab" && !e.shiftKey) {
        e.preventDefault();
        choose(matches[active]);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        dismissed = true;
        return;
      }
    }
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }
</script>

<div class="bar">
  <div class="field">
    <input
      bind:this={input}
      bind:value={query}
      type="search"
      placeholder="Search names, tag:name"
      aria-label="Search names"
      spellcheck="false"
      role="combobox"
      aria-autocomplete="list"
      aria-expanded={open}
      aria-controls="tag-complete"
      aria-activedescendant={open ? `tag-complete-${active}` : undefined}
      onkeydown={key}
      oninput={onInput}
      onkeyup={track}
      onclick={track}
      onfocus={() => {
        dismissed = false;
        loadKnown();
      }}
      onblur={() => (dismissed = true)}
    />
    {#if open}
      <ul class="complete" id="tag-complete" role="listbox" aria-label="Tags" bind:this={list}>
        {#each matches as name, i (name)}
          {@const cut = term ? Math.min(term.prefix.length, name.length) : 0}
          <li
            id="tag-complete-{i}"
            data-i={i}
            role="option"
            aria-selected={i === active}
            class:on={i === active}
            onmousedown={(e) => {
              e.preventDefault();
              choose(name);
            }}
            onmouseenter={() => (active = i)}
          >
            <b>{name.slice(0, cut)}</b>{name.slice(cut)}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
  <div class="seg-group" role="group" aria-label="Match">
    {#each modes as m (m.id)}
      <button type="button" class="seg" class:on={mode === m.id} aria-pressed={mode === m.id} title={m.title} onclick={() => (mode = m.id)}>{m.label}</button>
    {/each}
  </div>
  {#if showCase}
    <button type="button" class="case" class:on={caseSensitive} aria-pressed={caseSensitive} aria-label="Match case" title="Match case" onclick={() => (caseSensitive = !caseSensitive)}>Aa</button>
  {/if}
  <button
    type="button"
    class="case layout"
    aria-label={asList ? "Show results as tiles" : "Show results as a list"}
    title={asList ? "Show results as tiles" : "Show results as a list"}
    onclick={() => (asList = !asList)}
  >
    {#if asList}
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6">
        <rect x="2" y="2" width="5" height="5" rx="1" /><rect x="9" y="2" width="5" height="5" rx="1" /><rect x="2" y="9" width="5" height="5" rx="1" /><rect x="9" y="9" width="5" height="5" rx="1" />
      </svg>
    {:else}
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round">
        <path d="M2.5 4h11M2.5 8h11M2.5 12h11" />
      </svg>
    {/if}
  </button>
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
  .field {
    position: relative;
    flex: 1 1 280px;
    min-width: 0;
    display: flex;
  }
  input {
    flex: 1 1 auto;
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
  .complete {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    min-width: 220px;
    max-width: 100%;
    max-height: calc(8 * 28px + 8px);
    overflow-y: auto;
    margin: 0;
    padding: 4px;
    list-style: none;
    z-index: 20;
    border-radius: 8px;
    border: 1px solid var(--ctl-line);
    background: var(--ctl);
    box-shadow: 0 6px 18px rgb(0 0 0 / 0.25);
  }
  .complete li {
    height: 28px;
    line-height: 28px;
    padding: 0 10px;
    border-radius: 6px;
    color: var(--text);
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    cursor: pointer;
  }
  .complete li.on {
    background: var(--ctl-line);
    color: var(--text-strong);
  }
  .complete b {
    font-weight: 600;
    color: var(--text-strong);
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
  .layout {
    width: 34px;
    padding: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
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
