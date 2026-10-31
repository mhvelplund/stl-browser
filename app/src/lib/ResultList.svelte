<script lang="ts">
  import type { Hit } from "./api";
  import { listHits, type Rect } from "./band";
  import { bandGesture } from "./bandgesture";
  import { press, shouldStart, type Press } from "./dragstart";
  import { mods, NONE, type Mods } from "./selection";
  import KindIcon from "./KindIcon.svelte";
  import TagDots from "./TagDots.svelte";
  import { hitId, runs, where } from "./search";

  let {
    hits,
    scope,
    selected,
    onSelect,
    onBand,
    onClear,
    onOpen,
    onDragOut,
    onMenu,
  }: {
    hits: Hit[];
    scope: string[];
    /** The selected hits' `hitId`. */
    selected: ReadonlySet<string>;
    onSelect: (h: Hit, mods: Mods) => void;
    /** A rubber band's hits; `fresh` on a band's first call. */
    onBand: (ids: string[], ctrl: boolean, fresh: boolean) => void;
    /** A click on empty space. */
    onClear: () => void;
    /** Double-click or Enter: a folder opens in the browser, a file in its Windows app. */
    onOpen: (h: Hit) => void;
    onDragOut: (h: Hit) => void;
    onMenu: (h: Hit, x: number, y: number) => void;
  } = $props();

  let pressed: Press = null;
  let el: HTMLUListElement | undefined = $state();
  let box = $state<Rect | null>(null);

  /** Rows are equal height; measured from the first one (its top includes the list's padding). */
  function rowHits(r: Rect): string[] {
    const first = el?.querySelector<HTMLElement>("li:not(.band)");
    if (!first) return [];
    return listHits(r, hits.length, first.offsetHeight, first.offsetTop).map((i) => hitId(hits[i]));
  }

  const band = bandGesture(() => el, {
    startsOn: (t) => !t.closest(".row"),
    hits: rowHits,
    onBand: (ids, ctrl, fresh) => onBand(ids, ctrl, fresh),
    onClear: () => onClear(),
    onRect: (r) => (box = r),
  });

  function pointerMove(h: Hit, e: PointerEvent) {
    if (!shouldStart(pressed, e.clientX, e.clientY, e.buttons)) return;
    pressed = null;
    if (h.kind !== "locked") onDragOut(h);
  }

  function contextMenu(h: Hit, e: MouseEvent) {
    e.preventDefault();
    onMenu(h, e.clientX, e.clientY);
  }

  function key(h: Hit, e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      onSelect(h, NONE);
      onOpen(h);
    }
  }
</script>

<ul
  class="list"
  bind:this={el}
  onscroll={() => band.scrolled()}
  onpointerdown={band.down}
  onpointermove={band.move}
  onpointerup={band.up}
  onpointercancel={band.cancel}
>
  {#each hits as h (hitId(h))}
    <li>
      <button
        type="button"
        class="row"
        class:selected={selected.has(hitId(h))}
        aria-pressed={selected.has(hitId(h))}
        title={h.real.join("\n")}
        onclick={(e) => onSelect(h, mods(e))}
        ondblclick={() => onOpen(h)}
        onkeydown={(e) => key(h, e)}
        onpointerdown={(e) => (pressed = press({ button: e.button, x: e.clientX, y: e.clientY, pointerId: e.pointerId }))}
        onpointermove={(e) => pointerMove(h, e)}
        onpointerup={() => (pressed = null)}
        onpointercancel={() => (pressed = null)}
        oncontextmenu={(e) => contextMenu(h, e)}
      >
        <span class="icon"><KindIcon kind={h.kind} fileKind={h.fileKind} /></span>
        <span class="name">{#each runs(h.name, h.marks) as r, i (i)}{#if r.hit}<mark>{r.text}</mark>{:else}{r.text}{/if}{/each}</span>
        <span class="where">{where(h.parent, scope)}</span>
        <TagDots tags={h.tags} inline />
      </button>
    </li>
  {/each}
  {#if box}
    <li class="band" aria-hidden="true" style:left="{box.x0}px" style:top="{box.y0}px" style:width="{box.x1 - box.x0}px" style:height="{box.y1 - box.y0}px"></li>
  {/if}
</ul>

<style>
  .band {
    position: absolute;
    z-index: 2;
    pointer-events: none;
    border: 1px solid var(--sel-line);
    background: color-mix(in srgb, var(--sel-line) 18%, transparent);
  }
  .list {
    position: relative;
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    margin: 0;
    padding: 8px 12px 16px;
    list-style: none;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 32px;
    padding: 0 10px;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    font-size: 13px;
    text-align: left;
    cursor: default;
  }
  .row:hover {
    background: var(--tile-hover);
  }
  .row.selected {
    background: var(--sel-bg);
    border-color: var(--sel-line);
  }
  .icon {
    flex: none;
    width: 18px;
    height: 18px;
    color: var(--art-ink);
  }
  .icon :global(svg) {
    width: 18px;
    height: 18px;
  }
  .name {
    flex: none;
    max-width: 60%;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  mark {
    background: transparent;
    color: var(--accent);
    font-weight: 600;
  }
  .where {
    flex: 1 1 auto;
    min-width: 0;
    color: var(--faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
