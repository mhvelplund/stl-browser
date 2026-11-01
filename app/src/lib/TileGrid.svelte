<script lang="ts" generics="T extends Item">
  // A virtualized grid: only the rows in view (plus a few) are in the DOM.
  import { untrack } from "svelte";
  import type { SvelteMap } from "svelte/reactivity";
  import type { Item, View } from "./api";
  import { gridHits, type Rect } from "./band";
  import { bandGesture } from "./bandgesture";
  import { current, type Extras } from "./carousel";
  import type { Mods } from "./selection";
  import { columns, revealTop, tileWidth, visibleRows } from "./grid";
  import Tile from "./Tile.svelte";

  let {
    items,
    view,
    selected,
    scroll,
    onSelect,
    onBand,
    onClear,
    onOpen,
    onScroll,
    urls,
    extras,
    chosen,
    onSlot,
    onVisible,
    onDragOut,
    onMenu,
    idOf = (i: T) => i.name,
    captionOf,
    marksOf,
    reveal = null,
  }: {
    items: T[];
    view: View;
    /** The selected tiles' `idOf`. */
    selected: ReadonlySet<string>;
    /** Scroll position to restore whenever `items` changes. */
    scroll: number;
    onSelect: (i: T, mods: Mods) => void;
    /** A rubber band's hits; `fresh` on a band's first call. */
    onBand: (ids: string[], ctrl: boolean, fresh: boolean) => void;
    /** A click on empty space. */
    onClear: () => void;
    onOpen: (i: T) => void;
    onScroll: (top: number) => void;
    urls: SvelteMap<number, string | null>;
    /** A file's own extra picture urls by entry id. */
    extras: Extras;
    /** Slots picked this session, by item key. */
    chosen: SvelteMap<string, number>;
    onSlot: (i: T, slot: number) => void;
    /** The item index range that is rendered, `[first, last)`. */
    onVisible: (r: { first: number; last: number }) => void;
    onDragOut: (i: T) => void;
    onMenu: (i: T, x: number, y: number) => void;
    /** A tile's identity among `items` (names repeat across search hits). */
    idOf?: (i: T) => string;
    /** Replaces a tile's subtitle. */
    captionOf?: (i: T) => string;
    /** Name ranges to highlight. */
    marksOf?: (i: T) => [number, number][];
    /** The `idOf` of a tile to scroll into view when `items` change (instead of `scroll`). */
    reveal?: string | null;
  } = $props();

  const GAP = 8;
  const PAD = 20;
  /** Name and subtitle below the square art, plus the tile's padding. */
  const TEXT = 56;

  let el: HTMLDivElement | undefined = $state();
  let width = $state(0);
  let height = $state(0);
  let top = $state(0);

  const inner = $derived(Math.max(0, width - 2 * PAD));
  const cols = $derived(columns(inner, 200, GAP));
  const tile = $derived(tileWidth(inner, cols, GAP));
  const rowHeight = $derived(tile + TEXT + GAP);
  const rows = $derived(Math.ceil(items.length / cols));
  const range = $derived(visibleRows(Math.max(0, top - PAD), height, rowHeight, rows));
  const visible = $derived(
    Array.from({ length: range.last - range.first }, (_, k) => {
      const r = range.first + k;
      return { r, tiles: items.slice(r * cols, (r + 1) * cols) };
    }),
  );

  let box = $state<Rect | null>(null);
  const band = bandGesture(() => el, {
    startsOn: (t) => !t.closest(".tile"),
    hits: (r) => gridHits(r, items.length, { cols, tile, gap: GAP, pad: PAD, rowHeight }).map((i) => idOf(items[i])),
    onBand: (ids, ctrl, fresh) => onBand(ids, ctrl, fresh),
    onClear: () => onClear(),
    onRect: (r) => (box = r),
  });

  const first = $derived(range.first * cols);
  const last = $derived(Math.min(items.length, range.last * cols));
  $effect(() => {
    onVisible({ first, last });
  });

  /** A tile to scroll to once the grid has its size (a fresh object per listing, so the same
   * id revealed twice still re-runs). */
  let toReveal: { id: string } | null = $state(null);

  $effect(() => {
    items;
    untrack(() => {
      if (el) el.scrollTop = scroll;
      top = scroll;
      toReveal = reveal ? { id: reveal } : null;
    });
  });

  // re-applied whenever the layout changes (a scrollbar appearing drops a column), until the
  // user scrolls
  $effect(() => {
    if (!toReveal || !el || !width || !height) return;
    const want = toReveal.id;
    const index = untrack(() => items.findIndex((i) => idOf(i) === want));
    const t = revealTop(index, cols, rowHeight, PAD, height, untrack(() => top));
    if (t === untrack(() => top)) return;
    el.scrollTop = t;
    top = el.scrollTop; // the browser clamps it to the content
    onScroll(top);
  });
</script>

<!-- the rubber band is a mouse extra; keyboard users have Ctrl+A and the tiles' own buttons -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="scroller"
  bind:this={el}
  bind:clientWidth={width}
  bind:clientHeight={height}
  onscroll={() => {
    // a scroll that isn't the reveal's own is the user's: stop revealing
    if (Math.abs(el!.scrollTop - top) > 1) toReveal = null;
    top = el!.scrollTop;
    onScroll(top);
    band.scrolled();
  }}
  onpointerdown={band.down}
  onpointermove={band.move}
  onpointerup={band.up}
  onpointercancel={band.cancel}
>
  <div class="spacer" style:height="{rows * rowHeight - GAP + 2 * PAD}px">
    {#each visible as row (row.r)}
      <div
        class="row"
        style:top="{PAD + row.r * rowHeight}px"
        style:grid-template-columns="repeat({cols}, {tile}px)"
        style:height="{rowHeight - GAP}px"
      >
        {#each row.tiles as item (idOf(item))}
          <Tile
            {item}
            {view}
            selected={selected.has(idOf(item))}
            slot={current(item, chosen, extras)}
            caption={captionOf?.(item)}
            marks={marksOf?.(item)}
            {urls}
            {extras}
            onSelect={(i, m) => onSelect(i as T, m)}
            onOpen={(i) => onOpen(i as T)}
            onSlot={(i, s) => onSlot(i as T, s)}
            onDragOut={(i) => onDragOut(i as T)}
            onMenu={(i, x, y) => onMenu(i as T, x, y)}
          />
        {/each}
      </div>
    {/each}
    {#if box}
      <div class="band" style:left="{box.x0}px" style:top="{box.y0}px" style:width="{box.x1 - box.x0}px" style:height="{box.y1 - box.y0}px"></div>
    {/if}
  </div>
</div>

<style>
  .scroller {
    position: relative;
    user-select: none; /* a rubber band would select the names */
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
  }
  .spacer {
    position: relative;
  }
  .band {
    position: absolute;
    z-index: 2;
    pointer-events: none;
    border: 1px solid var(--sel-line);
    background: color-mix(in srgb, var(--sel-line) 18%, transparent);
  }
  .row {
    position: absolute;
    left: 20px;
    right: 20px;
    display: grid;
    gap: 8px;
  }
</style>
