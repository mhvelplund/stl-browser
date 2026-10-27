<script lang="ts">
  import type { Item, View } from "./api";
  import { subtitle } from "./items";
  import KindIcon from "./KindIcon.svelte";

  let {
    item,
    view,
    selected,
    onSelect,
    onOpen,
  }: { item: Item; view: View; selected: boolean; onSelect: (i: Item) => void; onOpen: (i: Item) => void } = $props();

  const badge = $derived(item.badge ?? (item.kind === "model" ? "MODEL" : null));

  function key(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      onSelect(item);
      if (item.openable) onOpen(item);
    }
  }
</script>

<button
  type="button"
  class="tile"
  class:selected
  class:locked={item.kind === "locked"}
  aria-label={item.name + (item.openable ? ", folder" : "")}
  aria-pressed={selected}
  title={item.real.join("\n")}
  onclick={() => onSelect(item)}
  ondblclick={() => item.openable && onOpen(item)}
  onkeydown={key}
>
  <span class="art">
    <KindIcon kind={item.kind} fileKind={item.fileKind} />
    {#if badge}<span class="badge">{badge}</span>{/if}
  </span>
  <span class="name">{item.name}</span>
  <span class="sub">{subtitle(item, view)}</span>
</button>

<style>
  .tile {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    padding: 10px;
    border: 1px solid transparent;
    border-radius: 12px;
    background: transparent;
    text-align: left;
    cursor: default;
    min-width: 0;
  }
  .tile:hover {
    background: var(--tile-hover);
    border-color: var(--line);
  }
  .tile.selected {
    background: var(--sel-bg);
    border-color: var(--sel-line);
  }
  .art {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    aspect-ratio: 1 / 1;
    border-radius: 8px;
    background: var(--art);
    color: var(--art-ink);
    flex: none;
  }
  .locked .art {
    color: var(--warn);
  }
  .badge {
    position: absolute;
    left: 8px;
    top: 8px;
    padding: 2px 7px;
    border-radius: 4px;
    background: var(--chip);
    color: var(--chip-text);
    font: 500 10px var(--mono);
    letter-spacing: 0.04em;
    text-transform: uppercase;
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
