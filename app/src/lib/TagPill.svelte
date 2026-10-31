<script lang="ts">
  import type { Tag } from "./api";

  let {
    tag,
    removable = false,
    disabled = false,
    onremove,
    faded = false,
    count,
    onpick,
  }: {
    tag: Tag;
    removable?: boolean;
    /** Show the × but don't let it remove. */
    disabled?: boolean;
    onremove?: (t: Tag) => void;
    /** Only some of the selected items have it. */
    faded?: boolean;
    /** Tooltip, e.g. "2 of 5". */
    count?: string;
    /** A click on the name (a faded pill: give it to the rest). */
    onpick?: (t: Tag) => void;
  } = $props();
</script>

<span class="pill {tag.kind}" class:faded title={count}>
  {#if onpick}<button type="button" class="text pick" {disabled} title={count ? `${count} · add to all` : "Add to all"} onclick={() => onpick(tag)}
      >{tag.name}</button
    >{:else}<span class="text">{tag.name}</span>{/if}{#if tag.byUser && tag.kind !== "user"}<span class="by" title="Set by you" aria-label="set by you">✎</span>{/if}{#if removable}<button
      type="button"
      class="x"
      {disabled}
      aria-label={"Remove " + tag.name}
      onclick={(e) => {
        e.stopPropagation();
        onremove?.(tag);
      }}>×</button
    >{/if}
</span>

<style>
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    max-width: 100%;
    box-sizing: border-box;
    padding: 2px 8px;
    border-radius: 10px;
    font: 500 11px/1.4 var(--font);
    white-space: nowrap;
  }
  .text {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .faded {
    opacity: 0.45;
  }
  .faded:hover {
    opacity: 0.7;
  }
  .pick {
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .pick:disabled {
    cursor: default;
  }
  .user {
    background: var(--tag-user);
    color: var(--tag-user-text);
  }
  .support {
    background: var(--tag-support);
    color: var(--tag-support-text);
  }
  .scale {
    background: var(--tag-scale);
    color: var(--tag-scale-text);
  }
  .creator {
    background: var(--tag-creator);
    color: var(--tag-creator-text);
  }
  .edition {
    background: var(--tag-edition);
    color: var(--tag-edition-text);
  }
  .by {
    font-size: 10px;
  }
  .x {
    margin: 0 -4px 0 0;
    padding: 0 4px;
    border: 0;
    background: transparent;
    color: inherit;
    font-size: 13px;
    line-height: 1;
    cursor: pointer;
    opacity: 0.75;
  }
  .x:hover:not(:disabled) {
    opacity: 1;
  }
  .x:disabled {
    opacity: 0.35;
    cursor: default;
  }
</style>
