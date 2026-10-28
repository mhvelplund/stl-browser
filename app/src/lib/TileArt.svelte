<script lang="ts">
  // The square art of a tile: a thumbnail, a 2×2 composite, a placeholder while generating, or
  // the kind icon when there is no thumbnail.
  import type { SvelteMap } from "svelte/reactivity";
  import type { Slot } from "./carousel";
  import KindIcon from "./KindIcon.svelte";

  let {
    slot,
    urls,
    kind,
    fileKind,
  }: { slot: Slot | null; urls: SvelteMap<number, string | null>; kind: string; fileKind: string | null } = $props();

  /** `undefined` = not known yet, `null` = no thumbnail. */
  const url = (id: number) => urls.get(id);
  const pending = $derived(
    slot !== null && (slot.kind === "pic" ? url(slot.thumb.entryId) === undefined : slot.cells.some((c) => url(c.entryId) === undefined)),
  );
  const single = $derived(slot?.kind === "pic" ? url(slot.thumb.entryId) : null);
</script>

{#if slot?.kind === "composite"}
  <span class="grid">
    {#each [0, 1, 2, 3] as i (i)}
      {@const cell = slot.cells[i]}
      <span class="cell">
        {#if cell && url(cell.entryId)}
          <img src={url(cell.entryId)} alt="" draggable="false" decoding="async" />
        {/if}
      </span>
    {/each}
  </span>
{:else if single}
  <img class="one" src={single} alt="" draggable="false" decoding="async" />
{:else if !pending}
  <KindIcon {kind} {fileKind} />
{/if}
{#if pending}
  <span class="skel"><span>generating thumbnail…</span></span>
{/if}

<style>
  .grid {
    position: absolute;
    inset: 0;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    grid-template-rows: repeat(2, minmax(0, 1fr));
    gap: 3px;
    background: var(--bg);
  }
  .cell {
    position: relative;
    background: var(--art);
    overflow: hidden;
  }
  img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  .one {
    padding: 6px;
    box-sizing: border-box;
  }
  .skel {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    padding-bottom: 14px;
    color: var(--muted);
    font: 400 11px var(--mono);
    animation: pulse 1s ease-in-out infinite alternate;
  }
  @keyframes pulse {
    from {
      background: color-mix(in srgb, var(--art) 92%, transparent);
    }
    to {
      background: color-mix(in srgb, var(--ctl-hover) 92%, transparent);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .skel {
      animation: none;
    }
  }
</style>
