# Architecture

[Overview](../README.md) · [Security](security.md) · [Connections](connections.md) · [Testing](testing.md)

## File layout

| Path | Responsibility |
| --- | --- |
| `src/routes/` | Application composition and SvelteKit entry points |
| `src/lib/components/` | Views grouped into `shell`, `chat`, `connection`, `storage` and shared `ui` |
| `src/lib/controllers/` | Workspace, setup, drafts and storage startup |
| `src/lib/stores/` | Chat/connection state, mutations and subscriptions |
| `src/lib/adapters/` | Tauri APIs, window actions and clipboard |
| `src/lib/models/` | Message, connection and storage types |
| `src/lib/presentation/` | Formatting, status labels and error text |
| `src/lib/styles/` | Theme tokens and shared CSS |
| `tests/frontend/` | Controller and store tests |

[+page.svelte](../src/routes/+page.svelte) injects APIs into controllers. Components
receive state/actions; Tauri calls stay in adapters. Svelte uses `$lib` imports;
TypeScript uses relative `.ts` imports for direct Node test execution.

## Runtime boundaries

| Layer | Main modules | Responsibility |
| --- | --- | --- |
| Frontend coordination | `chat-workspace`, `connection-setup`, `composer-draft`, `storage-controller` | Tabs, drafts, validation and readiness |
| Frontend state | `chat-store`, `connection-store` | Subscribe before snapshots; reject stale responses; refresh on focus |
| Rust domain | `message`, `incoming_message`, `identity`, `message_service`, `clock` | Validation, identities, message operations and retention rules |
| Persistence | `sqlite_repository`, `sqlite_identity`, `sqlite_peer`, `migrations` | Messages, profiles, pins and transactional upgrades |
| Security/startup | `credentials`, `encrypted_storage`, `storage_bootstrap`, `chat_state` | OS keys, cipher connections and blocking initialization |
| Transport | `peer/network`, `peer/transport`, `peer/protocol` | LAN/iroh streams and shared Noise framing |
| Sessions | `peer/coordinator`, `peer/delivery` | Reconnect, heartbeat, ACKs and retries |
| Platform | `commands`, `peer/commands`, `retention`, `startup_lock` | IPC, events, cleanup and instance coordination |

Rust modules live in `src-tauri/src/`. The coordinator has no Tauri/SQL dependency;
LAN and Internet reuse the same authenticated messaging and delivery logic.
OS calls run outside database locks, and locks do not cross awaits.

## Data invariants

- Chat mounts only after encrypted storage is ready; failures block startup with retry.
- Outgoing message and pending delivery commit together. ACKs persist before UI events.
- Incoming ordering/retention use local receipt time, otherwise creation time; insertion order breaks ties.
- Identical duplicates preserve receipt time; conflicting IDs fail. Deduplication lasts only while the row exists.
- Messages expire at exactly 24 hours; physical cleanup runs at startup and every 60 seconds. SQLite reuses pages without periodic VACUUM.
- Schema **6** stores a versioned connection profile and public Internet identity. Legacy upgrades preserve messages, IDs, pins and deliveries; invalid data fails without reset.
- Profile and peer pin commit before connect. Missing legacy LAN settings require setup once.

Schema definitions and history: [migrations.rs](../src-tauri/src/migrations.rs).
Encryption/recovery: [security](security.md).

## Window and interaction

- Opaque, always-on-top window; centered start/minimum size **350 × 500**, resizable; size/position are not saved.
- Header drag area, separate minimize/close actions; duplicate launches focus the existing instance.
- Enter sends, Shift+Enter adds a line break; failed saves preserve the draft.
- Tabs support arrows and Home/End; tab changes retain drafts, scroll position and active sessions.
- Message lists follow the bottom when appropriate and preserve reading position otherwise.
- DOM focus, keyboard behavior and scroll anchors stay in components.
