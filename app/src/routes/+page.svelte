<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  type AppInfo = { appDir: string; dbPath: string; schemaVersion: number };

  let info = $state<AppInfo | null>(null);
  let error = $state("");

  invoke<AppInfo>("app_info")
    .then((i) => (info = i))
    .catch((e) => (error = String(e)));
</script>

<div class="app">
  <main></main>
  <footer>
    {#if info}
      <span>Index: {info.dbPath}</span>
    {:else if error}
      <span class="error">Index unavailable: {error}</span>
    {/if}
  </footer>
</div>

<style>
  :global(body) {
    margin: 0;
    background: #15171a;
    color: #e8eaed;
    font-family: "IBM Plex Sans", system-ui, sans-serif;
  }
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  main {
    flex: 1;
  }
  footer {
    padding: 10px 20px;
    background: #1b1e22;
    border-top: 1px solid #2c3137;
    font-size: 12px;
    color: #a3aab3;
  }
  .error {
    color: #e8a0a0;
  }
</style>
