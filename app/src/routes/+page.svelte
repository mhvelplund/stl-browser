<script lang="ts">
  import "$lib/theme.css";
  import { onMount } from "svelte";
  import { SvelteMap } from "svelte/reactivity";
  import { open as pickFolder } from "@tauri-apps/plugin-dialog";
  import * as api from "$lib/api";
  import type { IndexDone, IndexProgress, Item, Listing, Status, View } from "$lib/api";
  import { requests, slotKey, slots } from "$lib/carousel";
  import { idle, step, type Run, type RunEvent } from "$lib/indexrun";
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
  let run = $state<Run>(idle);
  const busy = $derived(run.busy);
  let error = $state("");
  /** Scroll to restore when the next listing arrives. */
  let restore = $state(0);
  /** The grid's current scroll position (not reactive: only read when navigating). */
  let scroll = 0;
  let seq = 0;
  /** Thumbnail urls by entry id (`null` = no thumbnail); entry ids hold until an index run. */
  const urls = new SvelteMap<number, string | null>();
  /** Carousel slots picked in this listing, by item key. */
  const chosen = new SvelteMap<string, number>();
  let thumbPending = $state(0);
  let visible = { first: 0, last: 0 };
  let thumbTimer: ReturnType<typeof setTimeout> | undefined;

  /** Ask for the thumbnails of what is on screen first, then the rest of the folder (debounced). */
  function askThumbs() {
    clearTimeout(thumbTimer);
    thumbTimer = setTimeout(async () => {
      if (!listing) return;
      const r = requests(listing.items, chosen, visible);
      try {
        const got = await api.requestThumbs(r.high, r.low);
        for (const t of got.ready) urls.set(t.entryId, t.url);
        thumbPending = got.pending;
      } catch (e) {
        run = { ...run, hint: `Thumbnails unavailable: ${e}` };
      }
    }, 120);
  }

  function pickSlot(item: Item, i: number) {
    chosen.set(item.key, i);
    const s = slots(item)[i];
    if (s) api.selectThumb(item.key, slotKey(s)).catch((e) => (run = { ...run, hint: `Could not save the thumbnail choice: ${e}` }));
    askThumbs();
  }

  const usage = "Double-click a tile to open it · Backspace goes up · hover a tile to see where it really is";

  /** List `next.path`; a stale response (the user moved on) is dropped. */
  async function load(next: Nav, scrollTo = 0, keepSelection = false) {
    const my = ++seq;
    try {
      const l = await api.listFolder(view, next.path);
      if (my !== seq) return;
      nav = settle(next, l.path);
      listing = l;
      chosen.clear();
      askThumbs();
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
      run = { ...run, hint: `Status unavailable: ${e}` };
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

  const apply = (e: RunEvent) => (run = step(run, e));

  /** Start a run: the hint is set before the call, because `index-done` may beat its reply. */
  async function startRun(hint: string, call: () => Promise<unknown>) {
    apply({ type: "start", hint });
    try {
      await call();
      apply({ type: "accepted" });
    } catch (e) {
      apply({ type: "refused", error: String(e) });
    }
  }

  async function addFolder() {
    let path: string | string[] | null;
    try {
      path = await pickFolder({ directory: true, multiple: false, title: "Add a folder to the index" });
    } catch (e) {
      run = { ...run, hint: `Could not open the folder picker: ${e}` };
      return;
    }
    if (typeof path !== "string") return;
    const chosen = path;
    await startRun(`Indexing ${chosen}…`, () => api.addFolder(chosen));
    refreshStatus();
  }

  const rescan = () => startRun("Rescanning all folders…", api.rescan);

  function done(d: IndexDone) {
    progress = null;
    apply({ type: "done", done: d });
    urls.clear(); // entry ids change when sources are re-listed
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
  const footerHint = $derived(selectedItem ? selectedItem.real.join(" | ") : run.hint || usage);
  const noRoots = $derived(status !== null && status.roots.length === 0);

  onMount(() => {
    const unlisten = [
      api.onIndexProgress((p) => (progress = p)),
      api.onIndexDone(done),
      api.onThumbReady((r) => {
        urls.set(r.entryId, r.url);
        thumbPending = r.pending;
      }),
    ];
    refreshStatus().then(() => {
      if (status?.indexing && !run.busy) apply({ type: "start", hint: "Indexing…" });
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
          {urls}
          {chosen}
          onSlot={pickSlot}
          onVisible={(r) => {
            visible = r;
            askThumbs();
          }}
        />
      {:else}
        <p class="empty">{busy ? "Indexing…" : "This folder is empty"}</p>
      {/if}
    {/if}
  </main>
  <StatusBar {status} hint={footerHint} {progress} {thumbPending} />
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
