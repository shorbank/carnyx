<script lang="ts">
  import type { Snippet } from "svelte";
  let { id, label, children }: { id: string; label: string; children: Snippet } = $props();
</script>
<div class="field"><label for={id}>{label}</label>{@render children()}</div>
<style>.field { display: grid; gap: 6px; min-width: 0; } label { color: var(--muted); font-size: 12px; }</style>
