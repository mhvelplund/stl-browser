<script lang="ts" generics="T extends Item">
  // A virtualized grid: only the rows in view (plus a few) are in the DOM.
  import { untrack } from "svelte";
  import type { SvelteMap } from "svelte/reactivity";
  import type { Item, View } from "./api";
  import { current } from "./carousel";
  import { columns, revealTop, tileWidth, visibleRows } from "./grid";
  import Tile from "./Tile.svelte";

  let {
    items,
    view,
    selected,
    scroll,
    onSelect,
    onOpen,
    onScroll,
    urls,
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
    /** The selected tile's `idOf`. */
    selected: string | null;
    /** Scroll position to restore whenever `items` changes. */
    scroll: number;
    onSelect: (i: T) => void;
    onOpen: (i: T) => void;
    onScroll: (top: number) => void;
    urls: SvelteMap<number, string | null>;
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

  const first = $derived(range.first * cols);
  const last = $derived(Math.min(items.length, range.last * cols));
  $effect(() => {
    onVisible({ first, last });
  });

  /** A tile to scroll to once the grid has its size. */
  let toReveal: string | null = $state(null);

  $effect(() => {
    items;
    untrack(() => {
      if (el) el.scrollTop = scroll;
      top = scroll;
      toReveal = reveal;
    });
  });

  // re-applied whenever the layout changes (a scrollbar appearing drops a column), until the
  // user scrolls
  $effect(() => {
    if (!toReveal || !el || !width || !height) return;
    const want = toReveal;
    const index = untrack(() => items.findIndex((i) => idOf(i) === want));
    const t = revealTop(index, cols, rowHeight, PAD, height, untrack(() => top));
    if (t === untrack(() => top)) return;
    el.scrollTop = t;
    top = el.scrollTop; // the browser clamps it to the content
    onScroll(top);
  });
</script>

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
  }}
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
            selected={selected === idOf(item)}
            slot={current(item, chosen)}
            caption={captionOf?.(item)}
            marks={marksOf?.(item)}
            {urls}
            onSelect={(i) => onSelect(i as T)}
            onOpen={(i) => onOpen(i as T)}
            onSlot={(i, s) => onSlot(i as T, s)}
            onDragOut={(i) => onDragOut(i as T)}
            onMenu={(i, x, y) => onMenu(i as T, x, y)}
          />
        {/each}
      </div>
    {/each}
  </div>
</div>

<style>
  .scroller {
    position: relative;
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
  }
  .spacer {
    position: relative;
  }
  .row {
    position: absolute;
    left: 20px;
    right: 20px;
    display: grid;
    gap: 8px;
  }
</style>
