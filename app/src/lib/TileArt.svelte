<script lang="ts">
  // The square art of a tile: a thumbnail, a 2×2 composite, a placeholder while generating, or
  // the kind icon when there is no thumbnail.
  import type { SvelteMap } from "svelte/reactivity";
  import type { Extras, Slot } from "./carousel";
  import KindIcon from "./KindIcon.svelte";
  import { SPIN_DELAY_MS, SPIN_FRAMES, spinUrl } from "./spin";

  let {
    slot,
    urls,
    extras,
    kind,
    fileKind,
    hover = false,
  }: {
    slot: Slot | null;
    urls: SvelteMap<number, string | null>;
    /** A file's own extra picture urls by entry id (`sub` slots). */
    extras: Extras;
    kind: string;
    fileKind: string | null;
    /** The pointer is on the tile: a mesh thumbnail turns. */
    hover?: boolean;
  } = $props();

  /** `undefined` = not known yet, `null` = no thumbnail. */
  const url = (id: number) => urls.get(id);
  const pending = $derived(
    slot !== null &&
      (slot.kind === "pic"
        ? url(slot.thumb.entryId) === undefined
        : slot.kind === "composite" && slot.cells.some((c) => url(c.entryId) === undefined)),
  );
  // a `sub` slot only exists once its file's pictures have arrived
  const single = $derived(
    slot?.kind === "pic" ? url(slot.thumb.entryId) : slot?.kind === "sub" ? (extras.get(slot.thumb.entryId)?.[slot.n - 1] ?? null) : null,
  );

  // after a short rest, load the strip; it plays once loaded (a strip that isn't there yet, or
  // a thumbnail without one, leaves the still)
  const strip = $derived(spinUrl(slot, single));
  let playing: string | null = $state(null);
  $effect(() => {
    const s = strip;
    playing = null;
    if (!hover || !s) return;
    let live = true;
    const timer = setTimeout(() => {
      const img = new Image();
      img.onload = () => {
        if (live) playing = s;
      };
      img.src = s;
    }, SPIN_DELAY_MS);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  });
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
  {#if playing}
    <span class="spin" style:--frames={SPIN_FRAMES}><img src={playing} alt="" draggable="false" /></span>
  {/if}
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
  /* the strip's frames sit side by side; sliding it by whole frames turns the model */
  .spin {
    position: absolute;
    inset: 6px;
    overflow: hidden;
    pointer-events: none;
  }
  .spin img {
    inset: 0 auto auto 0;
    width: calc(var(--frames) * 100%);
    max-width: none;
    animation: turn 1.2s steps(var(--frames)) infinite;
  }
  @keyframes turn {
    to {
      transform: translateX(-100%);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .skel {
      animation: none;
    }
    .spin {
      display: none;
    }
  }
</style>
