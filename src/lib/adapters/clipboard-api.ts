export interface ClipboardApi { writeText(text: string): Promise<void> }
export const browserClipboardApi: ClipboardApi = {
  async writeText(text) {
    if (typeof navigator === "undefined" || !navigator.clipboard?.writeText) throw new Error("Clipboard unavailable");
    await navigator.clipboard.writeText(text);
  },
};
