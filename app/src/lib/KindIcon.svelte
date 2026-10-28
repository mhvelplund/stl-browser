<script lang="ts">
  // Placeholder art per item kind until thumbnails arrive (milestone 5).
  let { kind, fileKind }: { kind: string; fileKind: string | null } = $props();

  const shape = $derived.by(() => {
    if (kind === "locked") return "locked";
    if (kind === "model") return "model";
    if (kind === "archive") return "archive";
    if (kind === "volume") return "volume";
    if (kind === "image" || fileKind === "image") return "image";
    if (kind === "file") return fileKind === "model" || fileKind === "scene" ? "mesh" : "doc";
    return "folder";
  });
</script>

<svg viewBox="0 0 64 64" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round" stroke-linecap="round">
  {#if shape === "folder"}
    <path d="M8 18a3 3 0 0 1 3-3h13l5 5h24a3 3 0 0 1 3 3v24a3 3 0 0 1-3 3H11a3 3 0 0 1-3-3z" />
    <path d="M8 26h48" />
  {:else if shape === "volume"}
    <rect x="8" y="20" width="48" height="24" rx="4" />
    <path d="M8 34h48" />
    <circle cx="48" cy="39" r="1.6" fill="currentColor" />
  {:else if shape === "model"}
    <path d="M32 8 52 19v26L32 56 12 45V19z" />
    <path d="M12 19l20 11 20-11M32 30v26" />
  {:else if shape === "archive"}
    <path d="M10 22l22-10 22 10v22L32 54 10 44z" />
    <path d="M10 22l22 10 22-10M32 32v22" />
    <path d="M21 17l22 10v8" />
  {:else if shape === "locked"}
    <path d="M8 26l20-9 20 9v18L28 53 8 44z" />
    <path d="M8 26l20 9 20-9M28 35v18" />
    <rect x="40" y="38" width="18" height="15" rx="2.5" fill="var(--bg)" />
    <path d="M44 38v-4a5 5 0 0 1 10 0v4" />
    <circle cx="49" cy="45.5" r="1.6" fill="currentColor" />
  {:else if shape === "image"}
    <rect x="9" y="13" width="46" height="38" rx="3" />
    <circle cx="22" cy="25" r="4" />
    <path d="M9 45l14-12 10 9 7-6 15 12" />
  {:else if shape === "mesh"}
    <path d="M32 9 54 48H10z" />
    <path d="M32 9 26 48M32 9l6 39M18 34h28" />
  {:else}
    <path d="M16 8h22l10 10v38H16z" />
    <path d="M38 8v10h10M22 30h20M22 38h20M22 46h12" />
  {/if}
</svg>

<style>
  svg {
    width: 44%;
    height: 44%;
  }
</style>
