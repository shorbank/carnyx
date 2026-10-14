<script lang="ts">
  import type { createComposerDraft } from "./composer-draft";
  import Icon from "../ui/Icon.svelte";
  let { disabled = false, disabledReason = "", controller }: { disabled?: boolean; disabledReason?: string; controller: ReturnType<typeof createComposerDraft> } = $props();
  const draft = $derived(controller.state);
  let composing = $state(false);
  let input: HTMLTextAreaElement | undefined = $state();
  async function send() {
    if (composing || disabled) return;
    if (await controller.send()) input?.focus();
  }
  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" && !event.shiftKey && !composing && !event.isComposing && event.keyCode !== 229) {
      event.preventDefault(); void send();
    }
  }
  const id = $props.id();
</script>
<form onsubmit={(event) => { event.preventDefault(); void send(); }}>
  <label class="sr-only" for={id}>Nachricht</label>
  <div class="input-row">
    <textarea id={id} bind:this={input} value={$draft.text} rows="2" oninput={(event) => controller.setText(event.currentTarget.value)} placeholder="Nachricht schreiben …" aria-describedby={`${id}-hint`} onkeydown={handleKeydown} oncompositionstart={() => { composing = true; }} oncompositionend={() => { composing = false; }}></textarea>
    <button class="primary send" type="submit" aria-label={$draft.sending ? "Nachricht wird gespeichert" : "Nachricht senden"} title="Nachricht senden" disabled={disabled || !$draft.text.trim() || composing || $draft.sending}><Icon name={$draft.sending ? "pending" : "send"} /></button>
  </div>
  <small id={`${id}-hint`} class:unavailable={!!disabledReason}>{disabledReason || "Enter sendet · Shift+Enter: neue Zeile"}</small>
</form>
<style>
  form { padding: 10px 16px 18px; border-top: 1px solid var(--border); background: var(--background); }
  .input-row { display: flex; align-items: flex-end; gap: 8px; }
  textarea { flex: 1; background: var(--surface); padding: 10px; line-height: 1.5; }
  .send { display: grid; place-items: center; flex: 0 0 34px; width: 34px; height: 34px; padding: 0; border-radius: 8px; margin-bottom: 1px; }
  small { display: block; height: 0; visibility: hidden; color: var(--muted); font-size: 10px; }
  form:focus-within small, small.unavailable { height: auto; padding-top: 6px; visibility: visible; }
</style>
