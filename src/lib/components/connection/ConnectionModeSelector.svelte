<script lang="ts">
  import type { ConnectionMode } from "$lib/models/connection";
  let { mode, disabled, onchange }: { mode: ConnectionMode; disabled: boolean; onchange: (mode: ConnectionMode) => void } = $props();
</script>
<fieldset {disabled}><legend>Verbindungsart</legend>
  <label><input type="radio" name="connection-mode" checked={mode === "internet"} onchange={() => onchange("internet")} />Internet</label>
  <label><input type="radio" name="connection-mode" checked={mode === "lan"} onchange={() => onchange("lan")} />Lokales Netzwerk</label>
</fieldset>
<style>
  fieldset { display: flex; gap: 8px; margin: 0; padding: 0; border: 0; min-width: 0; flex-wrap: wrap; }
  legend { color: var(--muted); font-size: 12px; margin-bottom: 6px; }
  label { display: flex; align-items: center; gap: 7px; padding: 8px; border: 1px solid var(--border); border-radius: 8px; flex: 1; white-space: nowrap; background: var(--surface); cursor: pointer; font-size: 12px; }
  label:has(input:checked) { border-color: var(--accent); background: var(--surface-active); }
  input { appearance: none; width: 14px; height: 14px; border: 1px solid var(--muted); border-radius: 50%; margin: 0; flex: none; }
  input:checked { border: 4px solid var(--accent); }
</style>
