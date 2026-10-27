<script lang="ts">
  // A virtualized grid: only the rows in view (plus a few) are in the DOM.
  import { untrack } from "svelte";
  import type { Item, View } from "./api";
  import { columns, tileWidth, visibleRows } from "./grid";
  import Tile from "./Tile.svelte";

  let {
    items,
    view,
    selected,
    scroll,
    onSelect,
    onOpen,
    onScroll,
  }: {
    items: Item[];
    view: View;
    selected: string | null;
    /** Scroll position to restore whenever `items` changes. */
    scroll: number;
    onSelect: (i: Item) => void;
    onOpen: (i: Item) => void;
    onScroll: (top: number) => void;
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

  $effect(() => {
    items;
    untrack(() => {
      if (el) el.scrollTop = scroll;
      top = scroll;
    });
  });
</script>

<div
  class="scroller"
  bind:this={el}
  bind:clientWidth={width}
  bind:clientHeight={height}
  onscroll={() => {
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
        {#each row.tiles as item (item.name)}
          <Tile {item} {view} selected={selected === item.name} {onSelect} {onOpen} />
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
