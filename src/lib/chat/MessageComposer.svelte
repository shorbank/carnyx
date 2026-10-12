<script lang="ts">
  import { createComposerDraft } from "./composer-draft";
  let { onsend, disabled = false }: { onsend: (text: string) => Promise<boolean>; disabled?: boolean } = $props();
  const { state: draft, setText, send: save } = createComposerDraft((text) => onsend(text));
  let composing = $state(false);
  let input: HTMLTextAreaElement | undefined = $state();

  async function send() {
    if (composing || disabled) return;
    if (await save()) input?.focus();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" && !event.shiftKey &&
        !composing && !event.isComposing && event.keyCode !== 229) {
      event.preventDefault();
      void send();
    }
  }
</script>

<form onsubmit={(event) => { event.preventDefault(); void send(); }}>
  <label for="message-input">Nachricht</label>
  <textarea id="message-input" bind:this={input} value={$draft.text} rows="3"
    oninput={(event) => setText(event.currentTarget.value)}
    placeholder="Nachricht schreiben …" aria-describedby="message-hint"
    onkeydown={handleKeydown}
    oncompositionstart={() => { composing = true; }}
    oncompositionend={() => { composing = false; }}></textarea>
  <div class="actions">
    <small id="message-hint">Enter sendet · Shift+Enter: neue Zeile</small>
    <button type="submit" disabled={disabled || !$draft.text.trim() || composing || $draft.sending}>
      {$draft.sending ? "Speichert …" : "Senden"}
    </button>
  </div>
</form>

<style>
  form {
    display: grid;
    gap: 6px;
    padding: 12px;
    border-top: 1px solid rgba(255, 255, 255, 0.15);
  }

  label {
    font-size: 0.85rem;
  }

  textarea {
    box-sizing: border-box;
    width: 100%;
    min-width: 0;
    resize: none;
    border: 1px solid rgba(255, 255, 255, 0.35);
    border-radius: 8px;
    padding: 8px;
    background: rgba(0, 0, 0, 0.25);
    color: inherit;
    font: inherit;
  }

  textarea::placeholder {
    color: rgba(255, 255, 255, 0.7);
  }

  .actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  small {
    font-size: 0.7rem;
  }

  button {
    border: 1px solid rgba(255, 255, 255, 0.35);
    border-radius: 8px;
    padding: 8px 10px;
    background: rgba(0, 0, 0, 0.25);
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  textarea:focus-visible, button:focus-visible {
    outline: 2px solid white;
    outline-offset: 2px;
  }
</style>
