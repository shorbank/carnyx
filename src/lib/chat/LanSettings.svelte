<script lang="ts">
  import Field from "../ui/Field.svelte";
  import StatusNotice from "../ui/StatusNotice.svelte";
  let { addresses, address, port, disabled, onaddress, onport, onrefresh }: {
    addresses: string[]; address: string; port: string; disabled: boolean; onaddress: (value: string) => void; onport: (value: string) => void; onrefresh: () => void;
  } = $props();
</script>
<fieldset {disabled}><legend>Eigene Netzwerkadresse</legend>
  {#each addresses as value}<label><input type="radio" name="network-address" checked={address === value} onchange={() => onaddress(value)} /><span>{value}</span></label>{/each}
</fieldset>
{#if !addresses.length}<StatusNotice error text="Keine Netzwerkadresse verfügbar. Bitte deine Netzwerkverbindung prüfen." />{/if}
<Field id="network-port" label="Port"><input id="network-port" type="text" inputmode="numeric" value={port} oninput={(event) => onport(event.currentTarget.value)} {disabled} /></Field>
<button type="button" {disabled} onclick={onrefresh}>Netzwerkadressen neu laden</button>
<style>
  fieldset { margin: 0; padding: 0; border: 0; display: grid; gap: 6px; min-width: 0; }
  legend { color: var(--muted); font-size: 12px; margin-bottom: 8px; }
  label { display: flex; align-items: center; gap: 10px; padding: 9px 10px; background: var(--surface); border: 1px solid var(--border); border-radius: 8px; cursor: pointer; }
  label:has(input:checked) { border-color: var(--accent); background: var(--surface-active); }
  input[type="radio"] { appearance: none; width: 14px; height: 14px; margin: 0; border: 1px solid var(--muted); border-radius: 50%; background: var(--background); }
  input[type="radio"]:checked { border: 4px solid var(--accent); }
</style>
