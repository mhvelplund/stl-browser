<script lang="ts">
  import type { SvelteMap } from "svelte/reactivity";
  import type { Item, View } from "./api";
  import { slots, step } from "./carousel";
  import { subtitle } from "./items";
  import TileArt from "./TileArt.svelte";

  let {
    item,
    view,
    selected,
    slot,
    urls,
    onSelect,
    onOpen,
    onSlot,
  }: {
    item: Item;
    view: View;
    selected: boolean;
    /** The slot shown now. */
    slot: number;
    urls: SvelteMap<number, string | null>;
    onSelect: (i: Item) => void;
    onOpen: (i: Item) => void;
    onSlot: (i: Item, slot: number) => void;
  } = $props();

  const badge = $derived(item.badge ?? (item.kind === "model" ? "MODEL" : null));
  const all = $derived(slots(item));

  function key(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      onSelect(item);
      if (item.openable) onOpen(item);
    }
  }

  function flip(e: MouseEvent, dir: 1 | -1) {
    e.stopPropagation();
    onSlot(item, step(slot, all.length, dir));
  }
</script>

<div class="tile" class:selected class:locked={item.kind === "locked"}>
  <button
    type="button"
    class="main"
    aria-label={item.name + (item.openable ? ", folder" : "")}
    aria-pressed={selected}
    title={item.real.join("\n")}
    onclick={() => onSelect(item)}
    ondblclick={() => item.openable && onOpen(item)}
    onkeydown={key}
  >
    <span class="art">
      <TileArt slot={all[slot] ?? null} {urls} kind={item.kind} fileKind={item.fileKind} />
      {#if badge}<span class="badge">{badge}</span>{/if}
      {#if all.length > 1}<span class="count">{slot + 1} / {all.length}</span>{/if}
    </span>
    <span class="name">{item.name}</span>
    <span class="sub">{subtitle(item, view)}</span>
  </button>
  {#if all.length > 1}
    <button type="button" class="arrow left" aria-label="Previous thumbnail" onclick={(e) => flip(e, -1)} ondblclick={(e) => e.stopPropagation()}>
      <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true"><path d="M10 3 5 8l5 5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" /></svg>
    </button>
    <button type="button" class="arrow right" aria-label="Next thumbnail" onclick={(e) => flip(e, 1)} ondblclick={(e) => e.stopPropagation()}>
      <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true"><path d="M6 3l5 5-5 5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" /></svg>
    </button>
  {/if}
</div>

<style>
  .tile {
    position: relative;
    width: 100%;
    height: 100%;
    min-width: 0;
    border: 1px solid transparent;
    border-radius: 12px;
    box-sizing: border-box;
  }
  .tile:hover {
    background: var(--tile-hover);
    border-color: var(--line);
  }
  .tile.selected {
    background: var(--sel-bg);
    border-color: var(--sel-line);
  }
  .main {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    padding: 9px;
    border: 0;
    border-radius: 12px;
    background: transparent;
    text-align: left;
    cursor: default;
    min-width: 0;
  }
  .art {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    aspect-ratio: 1 / 1;
    border-radius: 8px;
    overflow: hidden;
    background: var(--art);
    color: var(--art-ink);
    flex: none;
  }
  .locked .art {
    color: var(--warn);
  }
  .badge,
  .count {
    position: absolute;
    top: 8px;
    padding: 2px 7px;
    background: var(--chip);
    color: var(--chip-text);
    font: 500 10px var(--mono);
  }
  .badge {
    left: 8px;
    border-radius: 4px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .count {
    right: 8px;
    border-radius: 10px;
  }
  .arrow {
    position: absolute;
    /* centred on the square art: the tile is the art plus 76px of padding and text */
    top: calc(10px + (100% - 76px) / 2 - 14px);
    width: 28px;
    height: 28px;
    border-radius: 14px;
    border: 0;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--chip);
    color: var(--text-strong);
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.12s;
  }
  .arrow.left {
    left: 16px;
  }
  .arrow.right {
    right: 16px;
  }
  .tile:hover .arrow,
  .tile:focus-within .arrow {
    opacity: 1;
  }
  .arrow:hover {
    background: var(--accent);
    color: var(--on-accent);
  }
  .name,
  .sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 0 2px;
  }
  .name {
    font-size: 14px;
    font-weight: 500;
    line-height: 1.3;
  }
  .sub {
    font-size: 12px;
    color: var(--muted);
  }
</style>
