import { get, writable } from "svelte/store";

/** Owns draft revisions so a delayed save never clears newer edits. */
export function createComposerDraft(onsend: (text: string) => Promise<boolean>) {
  const state = writable({ text: "", sending: false });
  let revision = 0;
  return {
    state: { subscribe: state.subscribe },
    setText(text: string) {
      revision++;
      state.update((current) => ({ ...current, text }));
    },
    async send(): Promise<boolean> {
      const current = get(state);
      if (current.sending || !current.text.trim()) return false;
      const submittedRevision = revision;
      state.update((value) => ({ ...value, sending: true }));
      try {
        const saved = await onsend(current.text);
        if (saved && submittedRevision === revision) state.update((value) => ({ ...value, text: "" }));
        return saved;
      } catch {
        return false;
      } finally {
        state.update((value) => ({ ...value, sending: false }));
      }
    },
  };
}
