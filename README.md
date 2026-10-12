# Carnyx

A lightweight, transparent desktop overlay chat application built for direct peer-to-peer (P2P) communication without relying on a central message-storage server

---

## Tech Stack

- Desktop Framework: [Tauri v2](https://tauri.app/)
- Frontend: [SvelteKit](https://svelte.dev/) + [TypeScript](https://www.typescriptlang.org/) + Vite
- Styling: Tailwind CSS
- Backend / Storage: Rust

---

## Current Local Chat

The overlay supports a single local conversation persisted in SQLite through Rust.
Enter sends a message; Shift+Enter inserts a line break. The Send button also sends
and returns focus to the input. Messages display their local time. The list follows
new messages when already at the bottom and preserves the reading position when
scrolled up. Only the header moves the window.

Reloading or reopening the app restores messages younger than 24 hours. Expired
messages are deleted at startup and in batches every 60 seconds while the app is
running. Rust notifies the open chat when messages expire. Focusing the window
also refreshes history after standby or missed events.

Only successfully stored messages are shown. While saving, the input stays editable;
confirmation clears it only if it has not been edited since sending. Failed saves
preserve the current draft and show an error, without automatic resend. Initial
load failures can be retried. Outside Tauri, the chat shows a desktop-only message
instead of using temporary replacement storage. P2P and click-through are not implemented.
The UI components and per-view chat store live in `src/lib/chat/`.

On desktop, only one Carnyx instance runs at a time. Launching the app again
shows, restores, and focuses the existing chat window without resetting messages.
This uses Tauri's [Single Instance plugin](https://v2.tauri.app/plugin/single-instance/).
On macOS, an OS file lock also serializes simultaneous launches until the plugin
is ready. The lock contains no messages and is automatically released on exit or
crash. Failure to establish instance coordination stops startup.

Validation (Node.js 22.6+ for the TypeScript store tests):

```bash
npm test
npm run check
npm run build
```

## Rust Message Storage

Startup opens `messages.sqlite` in Tauri's application data directory and keeps a
message service in managed application state. On first use, a transaction creates
schema version 3: `messages(id, text, created_at, sender_id, recipient_id, direction,
received_at)` and a singleton `local_identity` record. Version 1 and 2 databases are
migrated transactionally: existing IDs, text, timestamps and insertion order are
preserved, and existing messages become local messages from the app's own identity. Unknown
versions, damaged schemas and initialization errors stop startup; databases are
never reset. SQLite reuses freed pages; no periodic VACUUM shrinks the file.

The modules in `src-tauri/src/` have separate responsibilities:

- `message`: trims outer whitespace, rejects empty text, and creates UUID-v4 IDs.
  Inner whitespace and Unicode are preserved. `clock` supplies Unix milliseconds
  and can be replaced in tests.
- `identity`: typed UUID-v4 peer IDs and a stable local app identity. `sqlite_identity`
  persists it independently of message retention. A missing or invalid identity in
  an existing version 3 database stops startup instead of silently replacing it.
- `incoming_message`: validates untrusted message metadata separately from storage,
  including the source peer, sender, recipient, UUID and timestamp.
- `message_service`: creates and stores a message through a repository interface,
  returning success only after storage succeeds. It also owns the 24-hour cutoff:
  messages expire at exactly 24 hours and are excluded immediately from reads.
  Separate internal paths create targeted outgoing messages and receive messages
  idempotently; they do not send data over a network.
- `sqlite_repository`: owns the SQLite connection, parameterized inserts, atomic
  insert-if-absent, ordered reads, and transacted deletion. `migrations` owns schema
  validation and transactional upgrades; `storage_error` describes storage failures.
  SQLite is bundled through `rusqlite`.
- `chat_state`: owns shared service access on blocking workers; locks never cross awaits.
- `commands`: thin `list_messages` and `send_message` adapters with camelCase DTOs
  and sanitized error codes. The frontend adapter isolates Tauri calls from the store.
- `retention`: runs one cleanup task and emits removed IDs outside the service lock.
  Failed passes are retried at the next interval; the task stops on app exit.

The store subscribes before loading and rejects outdated snapshots when sends or
expiry events intervene. Draft revisions prevent delayed saves from clearing newer
edits. The UI follows new messages near the bottom and anchors the reading position
when older messages disappear. The existing chat still creates only `local` messages
with its own sender ID and no recipient. DTOs expose `senderId`, `recipientId`,
`direction` and `receivedAt` alongside the existing fields; there are no new commands
or receive events, peer selection controls or changes to message appearance.

Incoming messages retain the sender's ID and creation time, but receive a separate
local receipt time. Ordering, display and 24-hour retention use receipt time when
present, otherwise creation time, with insertion order breaking timestamp ties.
An expression index supports this local-time query. An identical incoming message
returns the stored message without changing its receipt time. A reused ID with
different content is rejected; stored rows are never overwritten. This duplicate
protection lasts while the row exists: after retention removes it, the ID can be
accepted again. No permanent replay ledger is kept.

The local UUID identifies this database, not an authenticated device. It survives
message cleanup and app restarts; deleting the database creates a new identity.
Keys, authentication, discovery, transport, delivery acknowledgements and incoming
UI notifications remain future work. The receive API's source peer must eventually
come from the transport's verified connection rather than the message payload.

Rust tests use temporary directories and a controllable clock, covering migrations,
reopening, identity stability, incoming validation, duplicate/conflict handling,
concurrent insertions, boundary cases, failures, actual commands with Tauri's mock runtime, and
the cleanup event/shutdown lifecycle. Frontend tests use controlled adapter promises
to verify races and draft preservation. Run Rust checks from `src-tauri`:

```bash
cargo test --lib
cargo check
cargo fmt --check
```

Build the desktop app from the project root:

```bash
npm run tauri build -- --debug --bundles app
```

---

## Project Goals and Roadmap

### 1. UI and Widget Design (Frontend)

- [ ] Implement a semi-transparent glassmorphism chat UI
- [ ] Configure window click-through behavior (interact only with the chat window, pass clicks through the transparent background to the desktop)
- [x] Add message rendering and auto-scroll behavior

### 2. Backend and 24-Hour Cache (Rust)

- [x] Set up a local embedded database (`rusqlite`) in the application data directory
- [x] Implement the initial local message schema (`id`, `text`, `created_at`)
- [x] Add persistent peer identity, sender/recipient metadata and local/outgoing/incoming directions
- [x] Prepare validated, idempotent incoming message storage without adding transport
- [x] Purge expired messages at startup and every 60 seconds; update the open chat
- [x] Connect the local chat to Rust through Tauri Commands for loading and storing messages

### 3. P2P Networking (Core Layer)

- [ ] Implement direct peer-to-peer connection logic in Rust (TCP/UDP or libp2p/WebRTC)
- [ ] Handle connection handshakes and direct data streaming
- [ ] Bridge incoming P2P packets to the frontend via Tauri Events for real-time UI updates

---

## Recommended IDE Setup

[VS Code](https://code.visualstudio.com/) + [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer).

## Getting Started

1. Install dependencies:

   ```bash
   npm install
   ```

2. Run in development mode:

   ```bash
   npm run tauri dev
   ```

3. Build for production:
   ```bash
   npm run tauri build
   ```
