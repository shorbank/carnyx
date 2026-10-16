import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export interface WindowApi {
  drag(): Promise<void>;
  resize(): Promise<void>;
  minimize(): Promise<void>;
  close(): Promise<void>;
}
async function desktop(action: () => Promise<void>) {
  if (isTauri()) await action();
}
export const tauriWindowApi: WindowApi = {
  drag: () => desktop(() => getCurrentWindow().startDragging()),
  resize: () => desktop(() => getCurrentWindow().startResizeDragging("SouthEast")),
  minimize: () => desktop(() => getCurrentWindow().minimize()),
  close: () => desktop(() => getCurrentWindow().close()),
};
