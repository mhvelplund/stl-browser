<script lang="ts">
  import type { Tag } from "./api";
  import TagPill from "./TagPill.svelte";
  import { dotKinds, groupByKind, KIND_LABEL } from "./tags";

  let { tags, inline = false }: { tags: Tag[]; /** In a row, not over a tile's art. */ inline?: boolean } = $props();

  const kinds = $derived(dotKinds(tags));
  const groups = $derived(groupByKind(tags));
  const label = $derived(tags.map((t) => KIND_LABEL[t.kind] + ": " + t.name).join(", "));

  const GAP = 8;
  let host = $state<HTMLElement | null>(null);
  let pop = $state<HTMLElement | null>(null);
  let open = $state(false);
  let pos = $state({ left: 0, top: 0 });

  function show() {
    open = true;
  }

  // Place the popover under the dots, right-aligned, and keep it inside the window.
  $effect(() => {
    if (!open || !host || !pop) return;
    const r = host.getBoundingClientRect();
    const w = pop.offsetWidth;
    const h = pop.offsetHeight;
    let left = r.right - w;
    let top = r.bottom + 6;
    if (top + h > window.innerHeight - GAP) top = r.top - 6 - h;
    left = Math.max(GAP, Math.min(left, window.innerWidth - w - GAP));
    top = Math.max(GAP, Math.min(top, window.innerHeight - h - GAP));
    pos = { left, top };
  });
</script>

{#if kinds.length}
  <span
    class="dots"
    class:inline
    role="img"
    aria-label={label}
    title=""
    bind:this={host}
    onpointerenter={show}
    onpointerleave={() => (open = false)}
  >
    {#each kinds as k (k)}<span class="dot {k}"></span>{/each}
  </span>
  {#if open}
    <!-- an empty title stops the tile's path tooltip from showing over the popover -->
    <span class="pop" role="presentation" title="" bind:this={pop} style:left="{pos.left}px" style:top="{pos.top}px">
      {#each groups as [kind, ts] (kind)}
        <span class="group">
          <span class="label">{KIND_LABEL[kind]}</span>
          <span class="pills">{#each ts as t (t.name)}<TagPill tag={t} />{/each}</span>
        </span>
      {/each}
    </span>
  {/if}
{/if}

<style>
  .dots {
    position: absolute;
    right: 8px;
    bottom: 8px;
    display: flex;
    gap: 3px;
    padding: 3px;
    border-radius: 8px;
    background: var(--chip);
  }
  .dots.inline {
    position: static;
    flex: none;
    background: transparent;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.35);
  }
  .dot.user {
    background: var(--tag-user);
  }
  .dot.support {
    background: var(--tag-support);
  }
  .dot.scale {
    background: var(--tag-scale);
  }
  .dot.creator {
    background: var(--tag-creator);
  }
  .dot.edition {
    background: var(--tag-edition);
  }
  .pop {
    position: fixed;
    z-index: 50;
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-width: 260px;
    box-sizing: border-box;
    padding: 8px 10px;
    border: 1px solid var(--ctl-line);
    border-radius: 8px;
    background: var(--bar);
    color: var(--text);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35);
    pointer-events: none;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .label {
    font: 500 10px var(--mono);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .pills {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
</style>
