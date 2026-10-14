<script lang="ts">
  import Icon, { type IconName } from "./Icon.svelte";
  let { icon, label, onclick, disabled = false }: { icon: IconName; label: string; onclick: () => void; disabled?: boolean } = $props();
</script>
<button type="button" class="icon-button" aria-label={label} title={label} {onclick} {disabled}><Icon name={icon} /></button>
<style>
  .icon-button { display: grid; place-items: center; width: 34px; height: 34px; padding: 0; border: 0; background: transparent; color: var(--muted); border-radius: 6px; }
  .icon-button:hover { color: var(--text); }
</style>
