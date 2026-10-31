<script lang="ts">
  import * as api from "./api";
  import type { NodeTags, Tag, TagKind, View } from "./api";
  import { groupByKind, KIND_LABEL, removalAction } from "./tags";
  import TagPill from "./TagPill.svelte";

  let {
    view,
    path,
    name,
    title,
    onchange,
    onerror,
  }: {
    view: View;
    /** The folder holding the target (the folder itself when `name` is empty). */
    path: string[];
    name: string;
    title: string;
    /** Called after an edit, so the listing and any search reload. */
    onchange: () => void;
    onerror: (message: string) => void;
  } = $props();

  const SETTABLE: TagKind[] = ["user", "support", "scale", "creator"];

  let data = $state<NodeTags | null>(null);
  let known = $state<Tag[]>([]);
  let kind = $state<TagKind>("user");
  let text = $state("");
  let seq = 0;

  /** The library root can't be tagged. */
  const taggable = $derived(path.length > 0 || name !== "");
  const groups = $derived(data ? groupByKind(data.own) : []);
  const suggestions = $derived([...new Set(known.filter((t) => t.kind === kind).map((t) => t.name))].sort());

  async function reload() {
    const my = ++seq;
    if (!taggable) {
      data = null;
      return;
    }
    try {
      const d = await api.nodeTags(view, path, name);
      if (my === seq) data = d;
    } catch (e) {
      if (my === seq) {
        data = null;
        onerror(`Could not load tags: ${e}`);
      }
    }
  }

  async function loadKnown() {
    try {
      known = await api.allTags();
    } catch (e) {
      onerror(`Could not load the tag list: ${e}`);
    }
  }

  $effect(() => {
    // reload when the target changes
    void [view, path, name];
    reload();
  });

  $effect(() => {
    loadKnown();
  });

  async function edit(what: string, call: () => Promise<void>) {
    try {
      await call();
    } catch (e) {
      onerror(`Could not ${what}: ${e}`);
      return;
    }
    await reload();
    loadKnown();
    onchange();
  }

  function remove(t: Tag) {
    const call = removalAction(t) === "remove" ? api.removeTag : api.hideTag;
    return edit(`remove ${t.name}`, () => call(view, path, name, t.kind, t.name));
  }

  function restore(t: Tag) {
    return edit(`restore ${t.name}`, () => api.unhideTag(view, path, name, t.kind, t.name));
  }

  async function add(e: SubmitEvent) {
    e.preventDefault();
    const value = text.trim();
    if (!value) return;
    text = "";
    await edit(`add ${value}`, () => api.addTag(view, path, name, kind, value));
  }
</script>

<div class="panel">
  <h2 title={title}>{title}</h2>
  {#if !taggable}
    <p class="hint">Select a folder or a model to tag it.</p>
  {:else}
    <section>
      <h3>Tags</h3>
      {#if groups.length}
        {#each groups as [k, tags] (k)}
          <div class="row">
            <span class="kind">{KIND_LABEL[k]}</span>
            <span class="pills">
              {#each tags as t (t.name)}
                <TagPill tag={t} removable onremove={remove} />
              {/each}
            </span>
          </div>
        {/each}
      {:else}
        <p class="hint">No tags</p>
      {/if}
      <form class="add" onsubmit={add}>
        <select bind:value={kind} aria-label="Tag kind">
          {#each SETTABLE as k (k)}<option value={k}>{KIND_LABEL[k]}</option>{/each}
        </select>
        <input bind:value={text} list="tag-suggestions" placeholder="Add a tag" aria-label="Tag name" spellcheck="false" />
        <datalist id="tag-suggestions">
          {#each suggestions as s (s)}<option value={s}></option>{/each}
        </datalist>
        <button type="submit" disabled={!text.trim()}>Add</button>
      </form>
    </section>
    {#if data?.inherited.length}
      <section>
        <h3>From folders above</h3>
        <span class="pills">
          {#each data.inherited as i (i.tag.kind + ":" + i.tag.name + ":" + i.from.join("/"))}
            <span title={i.from.join(" › ") || "Library"}><TagPill tag={i.tag} /></span>
          {/each}
        </span>
      </section>
    {/if}
    {#if data?.hidden.length}
      <section>
        <h3>Hidden</h3>
        {#each data.hidden as t (t.kind + ":" + t.name)}
          <div class="row hidden">
            <TagPill tag={t} />
            <button type="button" class="restore" onclick={() => restore(t)}>Restore</button>
          </div>
        {/each}
      </section>
    {/if}
  {/if}
</div>

<style>
  .panel {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    font-size: 13px;
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .row {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .kind {
    flex: none;
    width: 54px;
    color: var(--muted);
    font-size: 12px;
  }
  .pills {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    min-width: 0;
  }
  .hidden {
    align-items: center;
    justify-content: space-between;
  }
  .hint {
    margin: 0;
    color: var(--muted);
  }
  .add {
    display: flex;
    gap: 4px;
    margin-top: 4px;
  }
  .add input {
    flex: 1 1 auto;
    min-width: 0;
  }
  .add select,
  .add input,
  .add button,
  .restore {
    height: 28px;
    box-sizing: border-box;
    border-radius: 6px;
    border: 1px solid var(--ctl-line);
    background: var(--ctl);
    color: var(--text);
    font-size: 12px;
  }
  .add input {
    padding: 0 6px;
  }
  .add button,
  .restore {
    padding: 0 10px;
    cursor: pointer;
  }
  .add button:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
