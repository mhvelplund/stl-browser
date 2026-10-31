<script lang="ts">
  import { untrack } from "svelte";
  import * as api from "./api";
  import type { EditOp, NodeTags, Owner, Tag, TagKind, Target, TargetTags, View } from "./api";
  import { groupByKind, KIND_LABEL, mergeTags, removalAction, type MergedTag } from "./tags";
  import TagPill from "./TagPill.svelte";

  let {
    view,
    path,
    name,
    title,
    targets,
    onchange,
    onerror,
    onowner,
  }: {
    view: View;
    /** The folder holding the target (the folder itself when `name` is empty). */
    path: string[];
    name: string;
    title: string;
    /** The selected items; with more than one the panel edits them all (`path` / `name` are unused). */
    targets: Target[];
    /** Called after an edit, so the listing and any search reload. */
    onchange: () => void;
    onerror: (message: string) => void;
    /** Show the owner of a node that shares its folder with it. */
    onowner: (owner: Owner) => void;
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
  /** Set when the target shares its folder with the node above: that one is tagged instead. */
  const owner = $derived(data?.owner ?? null);
  const suggestions = $derived([...new Set(known.filter((t) => t.kind === kind).map((t) => t.name))].sort());

  /** Several selected: their tags merged, shared ones solid and the rest faded. */
  const multi = $derived(targets.length > 1);
  /** Changes only when the targets do (a new array with the same items doesn't reload). */
  const targetsKey = $derived(JSON.stringify(targets));
  let many = $state<TargetTags[] | null>(null);
  const merged = $derived(many ? mergeTags(many) : null);
  const mergedGroups = $derived(merged ? (groupByKind(merged.tags) as [TagKind, MergedTag[]][]) : []);
  /** None of the selected items can be tagged here. */
  const allSkipped = $derived(!!merged && merged.total === 0);
  const locked = $derived(multi ? allSkipped : !!owner);

  async function reload() {
    const my = ++seq;
    if (multi) {
      try {
        const d = await api.nodesTags(view, targets);
        if (my === seq) many = d;
      } catch (e) {
        if (my === seq) {
          many = null;
          onerror(`Could not load tags: ${e}`);
        }
      }
      return;
    }
    many = null;
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
    void [view, path, name, targetsKey];
    untrack(reload);
  });

  $effect(() => {
    loadKnown();
  });

  /** Run an edit; `true` when it went through. */
  async function edit(what: string, call: () => Promise<void>): Promise<boolean> {
    try {
      await call();
    } catch (e) {
      onerror(`Could not ${what}: ${e}`);
      return false;
    }
    await reload();
    loadKnown();
    onchange();
    return true;
  }

  function remove(t: Tag) {
    const call = removalAction(t) === "remove" ? api.removeTag : api.hideTag;
    return edit(`remove ${t.name}`, () => call(view, path, name, t.kind, t.name));
  }

  function restore(t: Tag) {
    return edit(`restore ${t.name}`, () => api.unhideTag(view, path, name, t.kind, t.name));
  }

  /** One edit over every selected item. */
  function bulk(what: string, op: EditOp, k: TagKind, value: string) {
    return edit(what, async () => {
      await api.editTags(view, targets, op, k, value);
    });
  }

  async function add(e: SubmitEvent) {
    e.preventDefault();
    const value = text.trim();
    if (!value) return;
    const call = multi ? () => bulk(`add ${value}`, "add", kind, value) : () => edit(`add ${value}`, () => api.addTag(view, path, name, kind, value));
    if (await call()) text = "";
  }

  const skippedHint = (n: number) => `${n} item${n === 1 ? " shares a folder" : "s share a folder"} with another item and ${n === 1 ? "is" : "are"} skipped`;
</script>

{#snippet addForm()}
  <form class="add" onsubmit={add}>
    <select bind:value={kind} aria-label="Tag kind" disabled={locked}>
      {#each SETTABLE as k (k)}<option value={k}>{KIND_LABEL[k]}</option>{/each}
    </select>
    <input bind:value={text} list="tag-suggestions" placeholder="Add a tag" aria-label="Tag name" spellcheck="false" disabled={locked} />
    <datalist id="tag-suggestions">
      {#each suggestions as s (s)}<option value={s}></option>{/each}
    </datalist>
    <button type="submit" disabled={locked || !text.trim()}>Add</button>
  </form>
{/snippet}

<div class="panel">
  <h2 title={multi ? undefined : title}>{multi ? `${targets.length} items` : title}</h2>
  {#if multi}
    {#if merged && merged.skipped > 0}
      <p class="hint">{skippedHint(merged.skipped)}</p>
    {/if}
    <section>
      <h3>Tags</h3>
      {#if mergedGroups.length}
        {#each mergedGroups as [k, tags] (k)}
          <div class="row">
            <span class="kind">{KIND_LABEL[k]}</span>
            <span class="pills">
              {#each tags as t (t.name)}
                <TagPill
                  tag={t}
                  removable
                  disabled={locked}
                  faded={t.faded}
                  count={t.faded ? `${t.count} of ${merged?.total}` : undefined}
                  onremove={(x) => bulk(`remove ${x.name}`, "remove", x.kind, x.name)}
                  onpick={t.faded && t.kind !== "edition" ? (x) => bulk(`add ${x.name}`, "fill", x.kind, x.name) : undefined}
                />
              {/each}
            </span>
          </div>
        {/each}
      {:else}
        <p class="hint">No tags</p>
      {/if}
      {@render addForm()}
    </section>
  {:else if !taggable}
    <p class="hint">Select a folder or a model to tag it.</p>
  {:else}
    {#if owner}
      {@const o = owner}
      <p class="hint">
        Shares its folder with <button type="button" class="link" title="Select {o.name}" onclick={() => onowner(o)}
          >{o.name}</button
        >; tag {o.name} instead
      </p>
    {/if}
    <section>
      <h3>Tags</h3>
      {#if groups.length}
        {#each groups as [k, tags] (k)}
          <div class="row">
            <span class="kind">{KIND_LABEL[k]}</span>
            <span class="pills">
              {#each tags as t (t.name)}
                <TagPill tag={t} removable disabled={!!owner} onremove={remove} />
              {/each}
            </span>
          </div>
        {/each}
      {:else}
        <p class="hint">No tags</p>
      {/if}
      {@render addForm()}
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
            <button type="button" class="restore" disabled={!!owner} onclick={() => restore(t)}>Restore</button>
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
  .add button:disabled,
  .add select:disabled,
  .add input:disabled,
  .restore:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    text-decoration: underline;
    cursor: pointer;
  }
</style>
