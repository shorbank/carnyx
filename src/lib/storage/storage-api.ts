import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { StorageApi, StorageStatus } from "./storage-controller";

export const tauriStorageApi: StorageApi = {
  status: () => invoke<StorageStatus>("get_storage_status"),
  retry: () => invoke<void>("retry_storage_init"),
  subscribe: (callback) => listen<StorageStatus>("storage_status_changed", (event) => callback(event.payload)),
};
