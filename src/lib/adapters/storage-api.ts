import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { StorageStatus } from "../models/storage.ts";

export interface StorageApi {
  status(): Promise<StorageStatus>;
  retry(): Promise<void>;
  subscribe(callback: (status: StorageStatus) => void): Promise<() => void>;
}

export const tauriStorageApi: StorageApi = {
  status: () => invoke<StorageStatus>("get_storage_status"),
  retry: () => invoke<void>("retry_storage_init"),
  subscribe: (callback) => listen<StorageStatus>("storage_status_changed", (event) => callback(event.payload)),
};
