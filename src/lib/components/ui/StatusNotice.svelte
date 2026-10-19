<script lang="ts">
  import type { Snippet } from "svelte";
  let { text, error = false, children }: { text: string; error?: boolean; children?: Snippet } = $props();
</script>
<div class="notice" class:error role={error ? "alert" : "status"}><p>{text}</p>{#if children}{@render children()}{/if}</div>
<style>
  .notice { display: grid; gap: 8px; padding: 10px 12px; border: 1px solid var(--border); border-radius: 8px; background: var(--surface); color: var(--muted); font-size: 12px; overflow-wrap: anywhere; }
  .error { color: var(--danger); }
</style>
