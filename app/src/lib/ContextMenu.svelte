<script lang="ts">
  // A small menu at the pointer. Escape, a press outside, scrolling or losing focus closes it.
  import { onMount } from "svelte";

  export type Entry = { label: string; action: () => void };

  let { x, y, entries, onClose }: { x: number; y: number; entries: Entry[]; onClose: () => void } = $props();

  let el: HTMLDivElement | undefined = $state();
  let left = $state(0);
  let top = $state(0);

  onMount(() => {
    // keep the menu inside the window
    const r = el!.getBoundingClientRect();
    left = Math.max(4, Math.min(x, window.innerWidth - r.width - 4));
    top = Math.max(4, Math.min(y, window.innerHeight - r.height - 4));
    el!.querySelector<HTMLButtonElement>("button")?.focus();
  });

  function pick(e: Entry) {
    onClose();
    e.action();
  }

  function key(e: KeyboardEvent) {
    const buttons = [...el!.querySelectorAll<HTMLButtonElement>("button")];
    const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onClose();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const n = buttons.length;
      buttons[(i + (e.key === "ArrowDown" ? 1 : n - 1)) % n]?.focus();
    }
  }

  function outside(e: PointerEvent) {
    if (el && !el.contains(e.target as Node)) onClose();
  }
</script>

<svelte:window onpointerdown={outside} onblur={onClose} onwheel={onClose} />

<div class="menu" role="menu" tabindex="-1" bind:this={el} style:left="{left}px" style:top="{top}px" onkeydown={key}>
  {#each entries as e (e.label)}
    <button type="button" role="menuitem" onclick={() => pick(e)}>{e.label}</button>
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 10;
    min-width: 160px;
    padding: 4px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--bar);
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.35);
    display: flex;
    flex-direction: column;
  }
  button {
    text-align: left;
    padding: 7px 12px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 13px;
    cursor: default;
  }
  button:hover,
  button:focus-visible {
    background: var(--sel-bg);
    outline: none;
  }
</style>
