<script lang="ts">
  import type { View } from "./api";
  import { crumbs, type Nav } from "./nav";

  let {
    nav,
    view,
    busy,
    onBack,
    onUp,
    onCrumb,
    onView,
    onAdd,
    onRescan,
    searching,
    onSearch,
  }: {
    nav: Nav;
    view: View;
    busy: boolean;
    onBack: () => void;
    onUp: () => void;
    onCrumb: (path: string[]) => void;
    onView: (v: View) => void;
    onAdd: () => void;
    onRescan: () => void;
    searching: boolean;
    onSearch: () => void;
  } = $props();

  const trail = $derived(crumbs(nav.path));
  let bar: HTMLElement | undefined = $state();

  // keep the current folder in view when the path is longer than the bar
  $effect(() => {
    trail;
    if (bar) bar.scrollLeft = bar.scrollWidth;
  });
</script>

<header>
  <div class="brand">
    <svg width="24" height="24" viewBox="0 0 26 26" aria-hidden="true" fill="none" stroke="var(--accent)" stroke-width="1.8" stroke-linejoin="round">
      <path d="M13 2 23 7.5v11L13 24 3 18.5v-11z" /><path d="M3 7.5 13 13l10-5.5M13 13v11" />
    </svg>
    <span>STL Browser</span>
  </div>
  <div class="group">
    <button type="button" class="btn icon" aria-label="Back" title="Back (Alt+Left)" disabled={!nav.back.length} onclick={onBack}>
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="M10 3 5 8l5 5" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" /></svg>
    </button>
    <button type="button" class="btn icon" aria-label="Up one folder" title="Up (Backspace, mouse Back)" disabled={!nav.path.length} onclick={onUp}>
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="M3 10l5-5 5 5" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" /></svg>
    </button>
  </div>
  <nav aria-label="Breadcrumb" class="crumbs" bind:this={bar}>
    {#each trail as c, i (i)}
      {#if i > 0}<span class="sep" aria-hidden="true">›</span>{/if}
      <button type="button" class="crumb" class:last={i === trail.length - 1} aria-current={i === trail.length - 1 ? "page" : undefined} onclick={() => onCrumb(c.path)}>{c.name}</button>
    {/each}
  </nav>
  <div class="seg-group" role="group" aria-label="View">
    <button type="button" class="seg" class:on={view === "models"} aria-pressed={view === "models"} onclick={() => onView("models")}>Models</button>
    <button type="button" class="seg" class:on={view === "files"} aria-pressed={view === "files"} onclick={() => onView("files")}>Files</button>
  </div>
  <div class="group">
    <button type="button" class="btn icon" class:on={searching} aria-label="Search" aria-pressed={searching} title="Search in this folder" onclick={onSearch}>
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><circle cx="7" cy="7" r="4.5" fill="none" stroke="currentColor" stroke-width="1.8" /><path d="M10.5 10.5 14 14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" /></svg>
    </button>
    <button
      type="button"
      class="btn icon"
      disabled={busy}
      aria-label={nav.path.length ? "Rescan folder" : "Rescan all"}
      title={nav.path.length ? "Rescan this folder and the folders in it" : "Rescan every indexed folder"}
      onclick={onRescan}
    >
      <span class="glyph" aria-hidden="true">🗘</span>
    </button>
    <button type="button" class="btn primary" disabled={busy} onclick={onAdd}>
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="M8 3v10M3 8h10" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" /></svg>
      Add folder
    </button>
  </div>
</header>

<style>
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    padding: 12px 20px;
    background: var(--bar);
    border-bottom: 1px solid var(--line);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-right: 8px;
    font-weight: 600;
    font-size: 15px;
  }
  .group {
    display: flex;
    gap: 6px;
  }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 14px;
    border-radius: 8px;
    border: 1px solid var(--ctl-line);
    background: var(--ctl);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
  }
  .btn:hover:not(:disabled) {
    background: var(--ctl-hover);
  }
  .btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .btn.icon {
    width: 36px;
    padding: 0;
    justify-content: center;
  }
  .btn.on {
    background: var(--ctl-line);
    color: var(--text-strong);
  }
  .glyph {
    font: 18px/1 "Segoe UI Symbol", sans-serif;
  }
  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--on-accent);
  }
  .btn.primary:hover:not(:disabled) {
    background: var(--accent-hover);
    border-color: var(--accent-hover);
  }
  .crumbs {
    display: flex;
    align-items: center;
    gap: 2px;
    flex: 1 1 320px;
    min-width: 0;
    height: 36px;
    padding: 0 6px;
    box-sizing: border-box;
    border-radius: 8px;
    background: var(--ctl);
    border: 1px solid var(--ctl-line);
    overflow-x: auto;
    scrollbar-width: none;
  }
  .sep {
    color: var(--faint);
    font-size: 12px;
  }
  .crumb {
    flex: none;
    border: 0;
    background: transparent;
    color: var(--muted);
    font-size: 13px;
    padding: 4px 6px;
    border-radius: 6px;
    cursor: pointer;
    white-space: nowrap;
  }
  .crumb:hover {
    background: var(--ctl-hover);
    color: var(--text);
  }
  .crumb.last {
    color: var(--text-strong);
    font-weight: 500;
  }
  .seg-group {
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: 8px;
    background: var(--ctl);
    border: 1px solid var(--ctl-line);
  }
  .seg {
    height: 30px;
    padding: 0 14px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--muted);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
  }
  .seg.on {
    background: var(--ctl-line);
    color: var(--text-strong);
  }
</style>
