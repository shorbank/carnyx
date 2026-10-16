<script lang="ts">
  import type { ChatState } from "$lib/stores/chat-store";
  import StatusNotice from "$lib/components/ui/StatusNotice.svelte";
  let { state, onretry }: { state: ChatState; onretry: () => Promise<void> } = $props();
</script>
{#if state.error}
  <div class="status"><StatusNotice error text={state.error.message}>{#if state.error.operation !== "send"}<button type="button" disabled={state.loading} onclick={() => { void onretry(); }}>Erneut versuchen</button>{/if}</StatusNotice></div>
{:else if state.loading && !state.loaded}
  <div class="status"><StatusNotice text="Nachrichten werden geladen …" /></div>
{/if}
<style>.status { padding: 10px 16px 0; }</style>
