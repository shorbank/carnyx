export type StorageFailure = "key_unavailable" | "key_missing" | "unlock_failed" | "cipher_unavailable" | "migration_failed" | "cleanup_failed" | "storage_unavailable" | "ipc_unavailable";
export interface StorageStatus {
  phase: "initializing" | "ready" | "blocked";
  code: StorageFailure | null;
  revision: number;
}
