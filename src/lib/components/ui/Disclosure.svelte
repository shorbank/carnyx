<script lang="ts">
  import type { Snippet } from "svelte";
  let { label, children }: { label: string; children: Snippet } = $props();
</script>
<details><summary>{label}<span aria-hidden="true">⌄</span></summary><div class="content">{@render children()}</div></details>
<style>
  details { border-top: 1px solid var(--border); }
  summary { list-style: none; display: flex; align-items: center; justify-content: space-between; padding: 12px 0; cursor: pointer; font-size: 12px; color: var(--muted); }
  summary::-webkit-details-marker { display: none; }
  details[open] summary span { transform: rotate(180deg); }
  .content { display: grid; gap: 12px; padding-bottom: 14px; min-width: 0; }
</style>
