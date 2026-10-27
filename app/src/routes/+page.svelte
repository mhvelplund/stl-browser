<script lang="ts">
  import "$lib/theme.css";
  import { onMount } from "svelte";
  import { open as pickFolder } from "@tauri-apps/plugin-dialog";
  import * as api from "$lib/api";
  import type { IndexDone, IndexProgress, Item, Listing, Status, View } from "$lib/api";
  import { folderSummary } from "$lib/items";
  import { goBack, goTo, open, settle, start, up, type Nav } from "$lib/nav";
  import StatusBar from "$lib/StatusBar.svelte";
  import TileGrid from "$lib/TileGrid.svelte";
  import Toolbar from "$lib/Toolbar.svelte";

  let view = $state<View>("models");
  let nav = $state<Nav>(start());
  let listing = $state<Listing | null>(null);
  let selected = $state<string | null>(null);
  let status = $state<Status | null>(null);
  let progress = $state<IndexProgress | null>(null);
  let busy = $state(false);
  let hint = $state("");
  let error = $state("");
  /** Scroll to restore when the next listing arrives. */
  let restore = $state(0);
  /** The grid's current scroll position (not reactive: only read when navigating). */
  let scroll = 0;
  let seq = 0;

  const usage = "Double-click a tile to open it · Backspace goes up · hover a tile to see where it really is";

  /** List `next.path`; a stale response (the user moved on) is dropped. */
  async function load(next: Nav, scrollTo = 0, keepSelection = false) {
    const my = ++seq;
    try {
      const l = await api.listFolder(view, next.path);
      if (my !== seq) return;
      nav = settle(next, l.path);
      listing = l;
      if (!keepSelection || !l.items.some((i) => i.name === selected)) selected = null;
      restore = scrollTo;
      scroll = scrollTo;
      error = "";
    } catch (e) {
      if (my === seq) error = String(e);
    }
  }

  async function refreshStatus() {
    try {
      status = await api.status();
    } catch (e) {
      hint = `Status unavailable: ${e}`;
    }
  }

  const openItem = (i: Item) => i.openable && load(open(nav, i.name, scroll));
  const goUp = () => nav.path.length && load(up(nav, scroll));
  const crumb = (path: string[]) => load(goTo(nav, path, scroll));
  function back() {
    const b = goBack(nav);
    if (b) load(b.nav, b.scroll);
  }
  function switchView(v: View) {
    if (v === view) return;
    view = v;
    load(start());
  }

  async function addFolder() {
    try {
      const path = await pickFolder({ directory: true, multiple: false, title: "Add a folder to the index" });
      if (typeof path !== "string") return;
      busy = true;
      const root = await api.addFolder(path);
      hint = `Indexing ${root.path}…`;
      await refreshStatus();
    } catch (e) {
      busy = false;
      hint = `Could not add the folder: ${e}`;
    }
  }

  async function rescan() {
    try {
      busy = true;
      await api.rescan();
      hint = "Rescanning all folders…";
    } catch (e) {
      busy = false;
      hint = `Could not rescan: ${e}`;
    }
  }

  function done(d: IndexDone) {
    busy = false;
    progress = null;
    const warn = d.warnings.length ? ` · ${d.warnings.length} warning(s): ${d.warnings.join("; ")}` : "";
    hint = d.ok
      ? `Indexed ${d.sources} sources · ${d.listed} listed · ${d.removed} removed · ${d.errors} errors · ` +
        `${d.models} models (${d.seconds.toFixed(1)} s)${warn}`
      : `Index failed: ${d.error}`;
    refreshStatus();
    load(nav, scroll, true);
  }

  function onKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null;
    if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
    if ((e.key === "Backspace" && !e.altKey) || (e.altKey && e.key === "ArrowUp")) {
      e.preventDefault();
      goUp();
    } else if (e.altKey && e.key === "ArrowLeft") {
      e.preventDefault();
      back();
    }
  }

  const selectedItem = $derived(listing?.items.find((i) => i.name === selected) ?? null);
  const footerHint = $derived(selectedItem ? selectedItem.real.join(" | ") : hint || usage);
  const noRoots = $derived(status !== null && status.roots.length === 0);

  onMount(() => {
    const unlisten = [api.onIndexProgress((p) => (progress = p)), api.onIndexDone(done)];
    refreshStatus().then(() => {
      if (status?.indexing) busy = true;
    });
    load(nav);
    return () => unlisten.forEach((u) => u.then((f) => f()));
  });
</script>

<svelte:window onkeydown={onKey} />

<div class="app">
  <Toolbar
    {nav}
    {view}
    {busy}
    onBack={back}
    onUp={goUp}
    onCrumb={crumb}
    onView={switchView}
    onAdd={addFolder}
    onRescan={rescan}
  />
  <main>
    {#if error}
      <p class="empty error">Could not list this folder: {error}</p>
    {:else if noRoots && !busy}
      <div class="empty">
        <h1>No folders yet</h1>
        <p>Add a folder of STL files or archives to start browsing.</p>
        <button type="button" class="add" onclick={addFolder}>Add folder</button>
      </div>
    {:else if listing}
      <div class="title" title={listing.real.join("\n")}>
        <h1>{listing.name || "Library"}</h1>
        <span>{folderSummary(listing, view)}</span>
      </div>
      {#if listing.items.length}
        <TileGrid
          items={listing.items}
          {view}
          {selected}
          scroll={restore}
          onSelect={(i) => (selected = i.name)}
          onOpen={openItem}
          onScroll={(t) => (scroll = t)}
        />
      {:else}
        <p class="empty">{busy ? "Indexing…" : "This folder is empty"}</p>
      {/if}
    {/if}
  </main>
  <StatusBar {status} hint={footerHint} {progress} />
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  main {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .title {
    display: flex;
    align-items: baseline;
    gap: 12px;
    padding: 20px 24px 0;
    min-width: 0;
  }
  .title h1 {
    margin: 0;
    font-size: 22px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .title span {
    color: var(--muted);
    font-size: 13px;
    white-space: nowrap;
  }
  .empty {
    margin: auto;
    text-align: center;
    color: var(--muted);
  }
  .empty h1 {
    color: var(--text);
    font-size: 22px;
    font-weight: 600;
    margin: 0 0 8px;
  }
  .error {
    color: var(--error);
  }
  .add {
    margin-top: 12px;
    height: 36px;
    padding: 0 16px;
    border-radius: 8px;
    border: 1px solid var(--accent);
    background: var(--accent);
    color: var(--on-accent);
    font-weight: 500;
    cursor: pointer;
  }
</style>
