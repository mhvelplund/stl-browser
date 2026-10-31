<script lang="ts">
  import "#lib/theme.css";
  import { onMount } from "svelte";
  import { SvelteMap } from "svelte/reactivity";
  import { open as pickFolder } from "@tauri-apps/plugin-dialog";
  import * as api from "#lib/api.ts";
  import type { ExtractProgress, Found, Hit, IndexDone, IndexProgress, Item, Listing, SearchMode, Status, Target, View } from "#lib/api.ts";
  import ContextMenu, { type Entry } from "#lib/ContextMenu.svelte";
  import { extractHint } from "#lib/extractrun.ts";
  import { requests, slotKey, slots } from "#lib/carousel.ts";
  import { idle, step, type Run, type RunEvent } from "#lib/indexrun.ts";
  import { folderSummary } from "#lib/items.ts";
  import { hitId, whereShort } from "#lib/search.ts";
  import { goBack, goTo, open, settle, start, up, type Nav } from "#lib/nav.ts";
  import { all, band, click, empty, focus, NONE, prune, type Mods, type Selection } from "#lib/selection.ts";
  import ResultList from "#lib/ResultList.svelte";
  import SearchBar from "#lib/SearchBar.svelte";
  import StatusBar from "#lib/StatusBar.svelte";
  import TagPanel from "#lib/TagPanel.svelte";
  import TileGrid from "#lib/TileGrid.svelte";
  import Toolbar from "#lib/Toolbar.svelte";

  let view = $state<View>("models");
  let nav = $state<Nav>(start());
  let listing = $state<Listing | null>(null);
  /** The selected tiles of the listing, by name. */
  let sel = $state<Selection>(empty);
  let status = $state<Status | null>(null);
  let progress = $state<IndexProgress | null>(null);
  let run = $state<Run>(idle);
  const busy = $derived(run.busy);
  let error = $state("");
  /** Scroll to restore when the next listing arrives. */
  let restore = $state(0);
  /** A tile to scroll into view when the next listing arrives (a file opened from search). */
  let reveal = $state<string | null>(null);
  /** The grid's current scroll position (not reactive: only read when navigating). */
  let scroll = 0;
  let seq = 0;
  /** Thumbnail urls by entry id (`null` = no thumbnail); entry ids hold until an index run. */
  const urls = new SvelteMap<number, string | null>();
  /** Carousel slots picked in this listing, by item key. */
  const chosen = new SvelteMap<string, number>();
  let thumbPending = $state(0);
  /** The right-click menu, if open. */
  let menu = $state<{ x: number; y: number; entries: Entry[] } | null>(null);
  let extracting = $state<ExtractProgress | null>(null);
  /** A result to show in the status bar until the next selection. */
  let notice = $state("");
  let visible = { first: 0, last: 0 };
  /** The search bar: open, its query and toggles, the folder it searches and what it found. */
  let searching = $state(false);
  let query = $state("");
  let mode = $state<SearchMode>("free");
  let caseSensitive = $state(false);
  let scope = $state<string[]>([]);
  let found = $state<Found | null>(null);
  let searchError = $state("");
  /** The selected search hits, by `hitId`. */
  let hitSel = $state<Selection>(empty);
  /** The selection a rubber band started from (Ctrl adds the band to it). */
  let bandBase: Selection = empty;
  /** Results as the compact list instead of tiles; remembered in this browser. */
  let asList = $state(readAsList());
  let searchSeq = 0;

  function readAsList(): boolean {
    try {
      return localStorage.getItem("results-as-list") === "1";
    } catch {
      return false;
    }
  }

  /** The tag side panel open; remembered in this browser. */
  let tagsOpen = $state(readTagPanel());

  function readTagPanel(): boolean {
    try {
      return localStorage.getItem("tagPanel") === "1";
    } catch {
      return false;
    }
  }

  $effect(() => {
    const v = tagsOpen ? "1" : "0";
    try {
      localStorage.setItem("tagPanel", v);
    } catch {
      // storage unavailable: the choice lasts for this session
    }
  });

  $effect(() => {
    const v = asList ? "1" : "0";
    try {
      localStorage.setItem("results-as-list", v);
    } catch {
      // storage unavailable: the choice lasts for this session
    }
  });
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  let thumbTimer: ReturnType<typeof setTimeout> | undefined;

  /** Ask for the thumbnails of what is on screen first, then the rest of the folder (debounced). */
  function askThumbs() {
    clearTimeout(thumbTimer);
    thumbTimer = setTimeout(async () => {
      const items = showingResults ? (asList ? [] : (found?.hits ?? [])) : listing?.items;
      if (!items) return;
      const r = requests(items, chosen, visible);
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

  const usage =
    "Double-click a tile to open it · Backspace or the mouse Back button goes up · hover a tile to see where it really is";

  /** List `next.path`; a stale response (the user moved on) is dropped. `show` names a tile to
   * scroll into view instead of restoring `scrollTo`. */
  async function load(next: Nav, scrollTo = 0, keepSelection = false, show: string | null = null) {
    const my = ++seq;
    try {
      const l = await api.listFolder(view, next.path);
      if (my !== seq) return;
      nav = settle(next, l.path);
      reveal = show;
      listing = l;
      chosen.clear();
      askThumbs();
      sel = keepSelection ? prune(sel, l.items.map((i) => i.name)) : empty;
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

  function openSearch() {
    scope = nav.path;
    searching = true;
  }

  function closeSearch() {
    clearTimeout(searchTimer);
    searchSeq++;
    searching = false;
    query = "";
    found = null;
    searchError = "";
    hitSel = empty;
  }

  /** Search the scope for the query (debounced); a stale reply (the user typed on) is dropped.
   * `keep`: a refresh (after an edit or an index run) keeps the selected hits that are still found. */
  function runSearch(keep = false) {
    clearTimeout(searchTimer);
    const my = ++searchSeq;
    if (!query) {
      found = null;
      searchError = "";
      return;
    }
    searchTimer = setTimeout(async () => {
      try {
        const f = await api.search(view, scope, query, mode, caseSensitive);
        if (my !== searchSeq) return;
        found = f;
        scope = f.path;
        searchError = "";
        hitSel = keep ? prune(hitSel, f.hits.map(hitId)) : empty;
        askThumbs();
      } catch (e) {
        if (my !== searchSeq) return;
        searchError = String(e);
        found = { path: scope, hits: [], truncated: false };
      }
    }, 120);
  }

  $effect(() => {
    // re-run on any change to the query or its toggles
    void [query, mode, caseSensitive];
    if (searching) runSearch();
  });

  // the results replace the grid: bring it back where it was scrolled to
  $effect(() => {
    if (showingResults) {
      restore = scroll;
      reveal = null; // a reveal is for the listing it came with, not for coming back from search
    }
  });

  /** Double-click or Enter on a hit closes search: a folder opens in the browser, a file is
   * shown selected in its folder. */
  function openHit(h: Hit) {
    closeSearch();
    if (h.openable) load(goTo(nav, [...h.parent, h.name], scroll));
    else showIn(h.parent, h.name);
  }

  /** Go to the folder at `path` with its tile `name` selected and scrolled into view. */
  function showIn(path: string[], name: string) {
    leaveSearch();
    sel = click(empty, name, [], NONE);
    load(goTo(nav, path, scroll), 0, true, name);
  }

  /** "Open" in a hit's menu: a folder opens in the browser, a file in its Windows app. */
  function launchHit(h: Hit) {
    if (h.openable) openHit(h);
    else if (h.kind !== "locked") launch(h, h.parent);
  }

  function selectHit(h: Hit, mods: Mods) {
    hitSel = click(hitSel, hitId(h), hitOrder, mods);
    notice = "";
  }

  /** A folder opens in the grid; a file in its Windows app. */
  function openItem(i: Item) {
    if (i.openable) load(open(nav, i.name, scroll));
    else if (i.kind !== "locked") launch(i);
  }

  /** The file being opened, if any: a second double-click doesn't extract it again. */
  let launching: string | null = null;
  async function launch(item: { name: string }, path = nav.path) {
    if (launching) return;
    launching = item.name;
    notice = `Opening ${item.name}…`;
    try {
      await api.openFile(view, path, item.name);
      notice = `Opened ${item.name}`;
    } catch (e) {
      notice = `Could not open ${item.name}: ${e}`;
    } finally {
      launching = null;
    }
  }

  /** Moving through the folders closes search first. */
  const leaveSearch = () => searching && closeSearch();
  function goUp() {
    leaveSearch();
    if (nav.path.length) load(up(nav, scroll));
  }
  function crumb(path: string[]) {
    leaveSearch();
    load(goTo(nav, path, scroll));
  }
  function back() {
    leaveSearch();
    const b = goBack(nav);
    if (b) load(b.nav, b.scroll);
  }
  /** Switch views, landing on the folder that matches this one. */
  let switching = 0;
  async function switchView(v: View) {
    const my = ++switching;
    if (v === view) return; // also drops a switch still waiting for `locate`
    leaveSearch();
    let path: string[] = [];
    try {
      path = await api.locate(view, nav.path, v);
    } catch {
      // the library root
    }
    if (my !== switching) return;
    view = v;
    load({ path, back: [] });
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

  /** Rescan the folder at `path` (everything at the library root). */
  function rescanAt(path: string[]) {
    const hint = path.length ? `Rescanning ${path.at(-1)}…` : "Rescanning all folders…";
    return startRun(hint, () => api.rescan(view, path));
  }
  const rescan = () => rescanAt(nav.path);

  /** Make a file the picture of the open folder. */
  async function useAsFolderThumb(item: Item) {
    if (!listing?.key) return;
    try {
      await api.selectThumb(listing.key, item.key);
      notice = `${item.name} is now the picture of ${listing.name}`;
    } catch (e) {
      notice = `Could not save the thumbnail choice: ${e}`;
    }
  }

  function select(i: Item, mods: Mods) {
    sel = click(sel, i.name, order, mods);
    notice = "";
  }

  /** A rubber band in the grid or the list: `fresh` marks its start. */
  function bandSelect(ids: string[], ctrl: boolean, fresh: boolean) {
    if (showingResults) {
      if (fresh) bandBase = hitSel;
      hitSel = band(bandBase, ids, ctrl);
    } else {
      if (fresh) bandBase = sel;
      sel = band(bandBase, ids, ctrl);
    }
    notice = "";
  }

  function clearSelection() {
    if (showingResults) hitSel = empty;
    else sel = empty;
  }

  /** The selected tiles (or hits) to drag or extract, locked ones left out. */
  function exportTargets(): Target[] {
    const items: { name: string; kind: string; parent?: string[] }[] = showingResults ? selectedHits : selectedItems;
    return items.filter((i) => i.kind !== "locked").map((i) => ({ path: i.parent ?? nav.path, name: i.name }));
  }

  /** Drag a tile out to Explorer, with the rest of the selection when it is selected: Rust runs
   * the OLE drag on the main thread. */
  function dragOut(item: Item) {
    sel = focus(sel, item.name);
    dragTargets(exportTargets());
  }

  function dragHit(h: Hit) {
    hitSel = focus(hitSel, hitId(h));
    dragTargets(exportTargets());
  }

  async function dragTargets(targets: Target[]) {
    if (!targets.length) return;
    try {
      const info = await api.beginDrag(view, targets);
      if (info.skipped) notice = `${info.skipped} item${info.skipped === 1 ? "" : "s"} left out of the drag (locked, or path too long)`;
    } catch (e) {
      notice = `Could not drag: ${e}`;
    }
  }

  async function extract(targets: Target[]) {
    if (!targets.length) return;
    const label = targets.length === 1 ? targets[0].name : `${targets.length} items`;
    let dest: string | string[] | null;
    try {
      dest = await pickFolder({ directory: true, multiple: false, title: `Extract ${label} to` });
    } catch (e) {
      notice = `Could not open the folder picker: ${e}`;
      return;
    }
    if (typeof dest !== "string") return;
    try {
      extracting = { index: 0, total: 0, name: label };
      await api.extractTo(view, targets, dest);
    } catch (e) {
      extracting = null;
      notice = `Could not extract: ${e}`;
    }
  }

  /** The right-click menu of a tile, or of a search hit (which offers only Open and Extract).
   * On a selection of several, only Extract to… (of them all). */
  function showMenu(item: Item | Hit, x: number, y: number) {
    const entries: Entry[] = [];
    const hit = "parent" in item ? item : null;
    if (hit) hitSel = focus(hitSel, hitId(hit));
    else sel = focus(sel, item.name);
    notice = "";
    const targets = exportTargets();
    if (selectedCount > 1) {
      menu = targets.length ? { x, y, entries: [{ label: "Extract to…", action: () => extract(targets) }] } : null;
      return;
    }
    if (item.kind !== "locked") entries.push({ label: "Open", action: () => (hit ? launchHit(hit) : openItem(item as Item)) });
    if (item.kind !== "locked") entries.push({ label: "Extract to…", action: () => extract(targets) });
    if (hit) {
      menu = entries.length ? { x, y, entries } : null;
      return;
    }
    const tile = item as Item;
    if (tile.openable && !busy) entries.push({ label: "Rescan", action: () => rescanAt([...nav.path, tile.name]) });
    if (!tile.openable && tile.pics.length && listing?.key) {
      entries.push({ label: "Use as folder thumbnail", action: () => useAsFolderThumb(tile) });
    }
    menu = entries.length ? { x, y, entries } : null;
  }

  function done(d: IndexDone) {
    progress = null;
    apply({ type: "done", done: d });
    urls.clear(); // entry ids change when sources are re-listed
    refreshStatus();
    load(nav, scroll, true);
    if (searching) runSearch(true); // the tree changed
  }

  function onKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null;
    if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT" || t.isContentEditable)) return;
    if (e.defaultPrevented) return; // the menu took its Escape
    if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "a") {
      e.preventDefault();
      if (showingResults) hitSel = all(hitOrder);
      else sel = all(order);
    } else if (e.key === "Escape") {
      clearSelection();
    } else if ((e.key === "Backspace" && !e.altKey) || (e.altKey && e.key === "ArrowUp")) {
      e.preventDefault();
      goUp();
    } else if (e.altKey && e.key === "ArrowLeft") {
      e.preventDefault();
      back();
    }
  }

  /** The mouse Back button (button 3) goes up; its default would be the webview's history. */
  function onMouse(e: MouseEvent) {
    if (e.button !== 3) return;
    e.preventDefault();
    if (e.type === "mouseup") {
      menu = null;
      goUp();
    }
  }

  const stop = (call: () => Promise<void>) => () =>
    call().catch((e) => (notice = `Could not stop: ${e}`));

  const showingResults = $derived(searching && query !== "");
  /** Display orders, for Shift ranges and Ctrl+A. */
  const order = $derived(listing?.items.map((i) => i.name) ?? []);
  const hitOrder = $derived(found?.hits.map(hitId) ?? []);
  const selectedItems = $derived(listing?.items.filter((i) => sel.ids.has(i.name)) ?? []);
  const selectedHits = $derived(found?.hits.filter((h) => hitSel.ids.has(hitId(h))) ?? []);
  const selectedCount = $derived(showingResults ? selectedHits.length : selectedItems.length);
  const selectedItem = $derived(selectedItems.length === 1 ? selectedItems[0] : null);
  const selectedHit = $derived(selectedHits.length === 1 ? selectedHits[0] : null);
  const shownReal = $derived(showingResults ? selectedHit?.real : selectedItem?.real);
  /** What the tag panel edits: the selected hit or item, else the open folder (nothing in results without a selection). */
  const tagTarget = $derived.by(() => {
    if (showingResults) return selectedHit ? { path: selectedHit.parent, name: selectedHit.name } : { path: [], name: "" };
    return { path: nav.path, name: selectedItem ? selectedItem.name : "" };
  });
  /** What the tag panel edits when several are selected. */
  const tagTargets = $derived<Target[]>(
    showingResults ? selectedHits.map((h) => ({ path: h.parent, name: h.name })) : selectedItems.map((i) => ({ path: nav.path, name: i.name })),
  );
  const tagTitle = $derived(
    showingResults ? (selectedHit?.name ?? "") : (selectedItem?.name ?? (listing?.name || "Library")),
  );

  /** After a tag edit the listing and any open search show the new tags. */
  function tagsChanged() {
    load(nav, scroll, true);
    if (searching) runSearch(true);
  }

  const footerHint = $derived(
    notice || (selectedCount > 1 ? `${selectedCount} selected` : shownReal ? shownReal.join(" | ") : run.hint || usage),
  );
  const noRoots = $derived(status !== null && status.roots.length === 0);

  onMount(() => {
    const unlisten = [
      api.onIndexProgress((p) => (progress = p)),
      api.onIndexDone(done),
      api.onExtractProgress((p) => (extracting = p)),
      api.onExtractDone((d) => {
        extracting = null;
        notice = extractHint(d);
      }),
      api.onThumbReady((r) => {
        urls.set(r.entryId, r.url);
        thumbPending = r.pending;
      }),
    ];
    refreshStatus().then(() => {
      if (status?.indexing && !run.busy) apply({ type: "start", hint: "Indexing…" });
      else if (status?.configWarning) run = { ...run, hint: status.configWarning };
    });
    load(nav);
    return () => unlisten.forEach((u) => u.then((f) => f()));
  });
</script>

<svelte:window onkeydown={onKey} onmousedown={onMouse} onmouseup={onMouse} />

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
    {searching}
    onSearch={() => (searching ? closeSearch() : openSearch())}
    {tagsOpen}
    onTags={() => (tagsOpen = !tagsOpen)}
  />
  {#if searching}
    <SearchBar
      bind:query
      bind:mode
      bind:caseSensitive
      bind:asList
      showCase={view === "files"}
      {scope}
      error={searchError}
      note={found?.truncated ? "showing first 1000" : ""}
      onClose={closeSearch}
    />
  {/if}
  <div class="body">
    <main>
      {#if showingResults}
        {#if found?.hits.length && !asList}
          <TileGrid
            items={found.hits}
            {view}
            idOf={hitId}
            selected={hitSel.ids}
            scroll={0}
            captionOf={(h) => whereShort(h.parent, scope)}
            marksOf={(h) => h.marks}
            onSelect={selectHit}
            onBand={bandSelect}
            onClear={clearSelection}
            onOpen={openHit}
            onScroll={() => (menu = null)}
            onDragOut={dragHit}
            onMenu={showMenu}
            {urls}
            {chosen}
            onSlot={pickSlot}
            onVisible={(r) => {
              visible = r;
              askThumbs();
            }}
          />
        {:else if found?.hits.length}
          <ResultList
            hits={found.hits}
            {scope}
            selected={hitSel.ids}
            onSelect={selectHit}
            onBand={bandSelect}
            onClear={clearSelection}
            onOpen={openHit}
            onDragOut={dragHit}
            onMenu={showMenu}
          />
        {:else if found && !searchError}
          <p class="empty">No matches</p>
        {/if}
      {:else if error}
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
            selected={sel.ids}
            scroll={restore}
            {reveal}
            onSelect={select}
            onBand={bandSelect}
            onClear={clearSelection}
            onOpen={openItem}
            onScroll={(t) => {
              scroll = t;
              menu = null;
            }}
            onDragOut={dragOut}
            onMenu={showMenu}
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
    {#if tagsOpen}
      <aside aria-label="Tags">
        <TagPanel
          {view}
          path={tagTarget.path}
          name={tagTarget.name}
          title={tagTitle}
          onchange={tagsChanged}
          onerror={(m) => (notice = m)}
          onowner={(o) => showIn(o.path, o.name)}
        />
      </aside>
    {/if}
  </div>
  <StatusBar
    {status}
    hint={footerHint}
    {progress}
    {thumbPending}
    extract={extracting}
    onStopIndex={stop(api.cancelIndex)}
    onStopExtract={stop(api.cancelExtract)}
  />
</div>
{#if menu}
  <ContextMenu x={menu.x} y={menu.y} entries={menu.entries} onClose={() => (menu = null)} />
{/if}

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  .body {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: row;
  }
  main {
    flex: 1 1 auto;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  aside {
    flex: none;
    width: 260px;
    overflow-y: auto;
    border-left: 1px solid var(--line);
    background: var(--bar);
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
