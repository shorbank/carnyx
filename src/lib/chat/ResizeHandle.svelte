<script lang="ts">
  import Icon from "../ui/Icon.svelte";
  let { onresize }: { onresize: () => void } = $props();
</script>
<div class="resize" role="img" aria-label="Fenstergröße am unteren rechten Rand ändern" title="Zum Vergrößern ziehen" onpointerdown={(event) => { if (event.button === 0) { event.preventDefault(); onresize(); } }}><Icon name="resize" size={16} /></div>
<style>.resize { position: absolute; bottom: 0; right: 0; width: 19px; height: 19px; display: grid; place-items: center; color: var(--muted); cursor: nwse-resize; user-select: none; } .resize:hover { color: var(--accent); }</style>
