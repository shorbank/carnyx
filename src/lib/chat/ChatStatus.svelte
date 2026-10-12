<script lang="ts">
  import type { ChatState } from "./chat-store";
  let { state, onretry }: { state: ChatState; onretry: () => Promise<void> } = $props();
</script>

{#if state.error}
  <div class="status" role="alert">
    <span>{state.error.message}</span>
    {#if state.error.operation !== "send"}
      <button type="button" disabled={state.loading} onclick={() => { void onretry(); }}>Erneut versuchen</button>
    {/if}
  </div>
{:else if state.loading && !state.loaded}
  <p class="status" role="status">Nachrichten werden geladen …</p>
{/if}

<style>
  .status {
    margin: 0;
    padding: 8px 12px;
    font-size: 0.8rem;
  }

  button {
    display: block;
    margin-top: 6px;
    padding: 6px;
    border-radius: 6px;
    border: 1px solid currentColor;
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
</style>
