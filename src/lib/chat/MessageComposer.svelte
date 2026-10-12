<script lang="ts">
  let { onsend }: { onsend: (text: string) => boolean } = $props();
  let draft = $state("");
  let composing = $state(false);
  let input: HTMLTextAreaElement | undefined = $state();

  function send() {
    if (composing) return;
    if (onsend(draft)) {
      draft = "";
      input?.focus();
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" && !event.shiftKey &&
        !composing && !event.isComposing && event.keyCode !== 229) {
      event.preventDefault();
      send();
    }
  }
</script>

<form onsubmit={(event) => { event.preventDefault(); send(); }}>
  <label for="message-input">Nachricht</label>
  <textarea id="message-input" bind:this={input} bind:value={draft} rows="3"
    placeholder="Nachricht schreiben …" aria-describedby="message-hint"
    onkeydown={handleKeydown}
    oncompositionstart={() => { composing = true; }}
    oncompositionend={() => { composing = false; }}></textarea>
  <div class="actions">
    <small id="message-hint">Enter sendet · Shift+Enter: neue Zeile</small>
    <button type="submit" disabled={!draft.trim() || composing}>Senden</button>
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
