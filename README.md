# Carnyx

A lightweight desktop overlay chat application built for direct peer-to-peer (P2P) communication without relying on a central message-storage server

---

## Tech Stack

- Desktop Framework: [Tauri v2](https://tauri.app/)
- Frontend: [SvelteKit](https://svelte.dev/) + [TypeScript](https://www.typescriptlang.org/) + Vite
- Styling: CSS with shared petrol-green theme tokens
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
load failures can be retried. Outside Tauri, the storage gate stays unavailable; no replacement history is created. The peer view supports one authenticated LAN or Internet peer on macOS and Windows. Click-through is not implemented.
The UI uses an opaque, flat petrol-green dark theme with no blur or shadows.
The **Lokal**, **Peer** and **Verbindung** tabs separate conversations from setup.
Tabs support arrow keys and Home/End; switching tabs preserves drafts and reading
positions. Connection setup guides code exchange and keeps network details and
identity information in expandable sections. Outgoing messages show compact delivery
icons with hover/focus explanations; these explanations can be dismissed with Escape.
The composer shows keyboard hints on focus.

The frameless window starts centered at 350 × 500, can be resized from its edges or
the lower-right grip, and cannot be made smaller than 350 × 500. The header has
separate minimize and close controls; only its title area moves the window. Size and
position are not persisted.

Components live in `src/lib/chat/`, shared UI elements and theme tokens in
`src/lib/ui/`. `chat-workspace.ts` owns conversation and draft lifetimes, tab selection,
send availability and focus refreshes. `connection-setup.ts` owns endpoint validation,
setup drafts and endpoint-bound code generation. Both are tested with injected APIs.
Tauri window actions are isolated in `window-api.ts`; view components do not invoke
Tauri commands directly.

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

## Encrypted Local History

`messages.sqlite` is encrypted as a whole with SQLCipher 4.19.0 (AES-256 page
protection and authentication), including message text, identities, peer pins,
session settings and delivery metadata. A random 32-byte history key lives in
macOS Keychain or Windows Credential Manager under
`<app identifier>.history-key.v1` / `messages.sqlite`. It is independent of the
existing Noise key; its credential service/account remain unchanged. Keys stay in
Rust, use zeroizing buffers and enter SQLCipher via `sqlite3_key`, never SQL or IPC.
Journal/WAL page payloads are encrypted; temporary SQL storage stays in memory.

Startup runs on a blocking worker. The UI subscribes to storage status before
reading its snapshot and only mounts the chat after readiness. OS-store errors,
missing/wrong keys, damaged data and failed conversion/cleanup show a German error
with a retry action. No chat, peer or retention services start while blocked.
No reset, replacement key for ciphertext, or plaintext fallback is available.

On first opening an old plaintext database, Carnyx creates a sibling encrypted
candidate (`messages.encrypted-pending.sqlite`) and a content-free progress marker
(`messages.encryption-v1`). It copies ordinary tables/indexes, including hidden
rowids and `user_version`, then reopens with the persisted key. It checks database
and foreign-key integrity, compares every row/schema/version, and validates the
existing schema upgrades in a transaction that is rolled back. Only then does it
retire the source (`messages.plaintext-retired.sqlite`), promote the candidate, and
remove the old file and sidecars. Partial copies are recreated from the intact
source; interruption between the two renames or during cleanup is resumed on retry.
Ambiguous or corrupt artifacts block startup. Encryption preserves the logical
database; schema upgrades are validated separately. Expiration begins after conversion and cleanup, preserving existing policy.
The conversion temporarily needs space for another full database.

This protects a copied database, not malware running as the unlocked OS user or
messages already in app memory. Deleting legacy files cannot securely erase old
SSD pages, filesystem snapshots or backups. Losing the OS key makes the encrypted
history unreadable; copying just the database to another machine is insufficient.
Key backup, password unlocking and rotation are outside this feature.

`credentials.rs` contains the native credential adapter; `encrypted_storage.rs`
contains key lifecycle and cipher connections; `encrypted_storage/migration.rs`
contains file conversion/recovery; `storage_bootstrap.rs` controls service startup.
`src/lib/storage/` separates the typed frontend controller/API from the gate view.
The native sources, provenance, licenses and reproduction instructions are in
`src-tauri/vendor/README.md`. macOS uses CommonCrypto; Windows builds static OpenSSL
and needs MSVC plus Perl. CI verifies source hashes and runs the full Rust tests on
both desktop platforms, including real encrypted temporary databases, wrong/missing
keys, old schemas, interrupted phases, cleanup failures, WAL/journal canaries and
existing peer/reconnect behavior. Native OS-store prompts, Windows execution and
cross-platform acceptance must be verified on their actual systems; tests use
injected credentials and never access users' keys or application databases.

## Rust Message Storage

Startup opens `messages.sqlite` in Tauri's application data directory and keeps a
message service in managed application state. On first use, a transaction creates
schema version 6: `messages(id, text, created_at, sender_id, recipient_id, direction,
received_at)` and a singleton `local_identity` record, plus transport identities and a typed connection profile. Version 1–5 databases are
migrated transactionally: existing IDs, text, timestamps and insertion order are
preserved. Version 1 and 2 messages become local messages from the app's own identity;
version 3 retains its existing directions and peer metadata. Unknown
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
  Separate paths create targeted outgoing messages and receive messages idempotently.
  The peer coordinator invokes them without putting network code into the service.
- `sqlite_repository`: owns the SQLite connection, parameterized inserts, atomic
  insert-if-absent, ordered reads, and transacted deletion. `migrations` owns schema
  validation and transactional upgrades; `storage_error` describes storage failures.
  SQLCipher is bundled through `rusqlite`; all production connections pass through
  `encrypted_storage` before the repository sees them.
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
`direction` and `receivedAt` alongside the existing fields; peer commands and change events connect the separate peer UI and coordinator.
Local and peer histories are displayed separately; listeners explicitly target the
main window so targeted message and expiry events reach the UI.

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
Transport keys and authenticated Noise sessions bind peer UUIDs to exchanged public
keys. The receive API gets its source peer from the verified session, not the payload.
Public peer discovery remains outside the feature. Internet connections use explicit
code exchange, NAT traversal and a fixed relay, described below.

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
npm run tauri build -- --bundles app
```

---

## Internet Peer Chat (Pilot)

Choose **Internet** in **Verbindung** on both devices. Show and exchange codes
through a trusted channel, paste the other device's code, and click **Verbinden**
on both devices. There is no IP, port or role selection. New setups default to
Internet; saved LAN setups keep their original mode. One peer is supported.

Carnyx uses pinned `iroh 1.3.0` with `tls-ring`, without its default features.
`peer/network.rs` owns LAN sockets and Internet endpoints behind the same ordered
bidirectional stream. `peer/transport.rs` applies the existing Noise handshake,
framing, limits and encrypted identity confirmation on either stream. The
coordinator retains the single delivery/retry implementation. The smaller canonical
Carnyx UUID dials and starts Noise; the other side accepts the expected transport
ID only. Incoming identity/Noise mismatches never enable sending.

The configured public pilot relay is `https://euc1-1.relay.n0.iroh.link./` (number0's
EU relay), centralized in `peer/types.rs`. Its address is included in v2 codes and
is checked against the configured relay when importing or connecting. No arbitrary
relay URL is accepted from a pasted code. iroh can upgrade from relay to a direct
path; **Netzwerkdetails** reports the selected path from connection events. Changes
of path do not restart the authenticated session or delivery state.

There is no public endpoint address lookup/publication, automatic router port
mapping, or automatic relay selection. Creating a code needs no network connection;
network resources are created only on explicit connect/reconnect and closed on
disconnect/exit. Tab changes and refreshes do not connect. Both applications must
have started their sessions; new messages still require an authenticated connection.
Unacknowledged messages survive locally and resume on reconnect within retention.
There is no server-side offline queue or push service.

The relay forwards encrypted traffic, but its operator can observe transport
identifiers, IPs and traffic metadata. The public relay has no Carnyx availability
guarantee. If its configured address changes, exchange new codes. No automatic
public discovery or relay failover is enabled in this pilot.

The separate Internet secret is stored under `<app identifier>.internet-key.v1`
with the local Carnyx UUID as account, using the shared native credential adapter.
Its public endpoint ID is stored in encrypted SQLite. Interrupted first setup can
recover the stored secret; a missing/mismatched existing secret is never replaced.
Internet key failures leave LAN and local chat available. Noise and history keys
are not reused or rotated.

Schema 6 transactionally replaces `trusted_peer` / `peer_session_settings` with a
versioned `connection_profile` singleton and adds `internet_identity`. Old LAN
selection and optional role/endpoint settings migrate together; messages, rowids,
Noise identity, remembered key pins and delivery records stay unchanged. Imported
v1 LAN codes retain their wire format. v2 codes carry Internet endpoint/relay
information; mismatched modes, invalid profiles and unknown versions fail explicitly.

Frontend mode/code/LAN components receive typed props and callbacks. The existing
setup controller owns per-mode drafts, validation and stale response protection;
the connection store owns mutations and lifecycle snapshots. UI components never
open endpoints or access keys.

Automated Internet tests use local relay servers and temporary encrypted databases,
including forced-relay delivery/ACK/reconnect, direct Noise streams, wrong pins,
unknown endpoints, cancellation and profile migration rollback. No public relay is
required in CI. An isolated macOS probe also passed the existing Noise handshake
and encrypted frames over the public EU relay with direct IP transports disabled.
Windows, real macOS–Windows two-router acceptance, network changes/standby and native
UI/credential prompt acceptance remain to be performed on actual devices.

## Authenticated LAN Peer Chat

Peer chat is supported on macOS and Windows. The **Lokal / Peer** switch separates
local history from the selected peer conversation and preserves each draft in memory.
Startup reads saved peer information and local network addresses without accessing
the secret store or opening sockets. Showing a connection code or starting a session
initializes a persistent X25519 key. Its private half lives
in macOS Keychain or Windows Credential Manager; SQLite stores only public keys.
Missing or mismatched keys are never silently replaced. Key-store errors leave the
local chat available. SQLCipher separately encrypts the complete SQLite database.

To connect two computers on the same network:

1. Open the **Verbindung** tab on both computers and choose **Lokales Netzwerk**. In **Netzwerkdetails**, select
   each computer's LAN IPv4 address and a port (default `32147`). Choose
   **Eigenen Code anzeigen**.
2. Exchange these public connection codes through a trusted channel and paste the
   other's code on each computer. Use **Code kopieren**, or mark and copy the code if automatic copying is unavailable.
   Codes contain the UUID, public key, address and port;
   they contain no private key. Do not accept codes from an untrusted source.
3. Choose **Auf Peer warten** on one computer and **Verbinden** on the other. Both
   codes must refer to the addresses and ports chosen in setup. The firewall must
   allow Carnyx at that endpoint; no automatic firewall changes are made.
4. Select **Peer** after setup. New peer messages require **Verschlüsselt verbunden**.
   **Verbindung trennen** stops the listener, reconnection and retries. Choosing the local view
   keeps an active peer connection running.

After a restart, the **Peer** tab restores the selected peer and its history without
requiring setup to be opened. **Erneut verbinden** reuses the saved local IPv4 address,
port and role: one device waits and the other connects. Both devices must start their
session explicitly; choosing a tab never connects automatically. **Einrichtung ändern**
opens the existing code-exchange form. Edited endpoint settings and drafts survive
refreshes and tab switches.

Schema 5 introduced `peer_session_settings`, linked by peer ID to the selected peer;
schema 6 migrates these fields into the common connection profile. Peer
selection, public-key pinning and session settings commit together before the
coordinator starts. These are the last validated requested settings, not a record of
successful authentication; a failed connection or occupied listener port leaves them
available for retry. Storage failures prevent startup of the session. Schema 4 users
retain their peer and messages but must complete setup once because the old database
contains no local endpoint or role. Migration failures never reset the database.

If the saved local address is no longer on this computer, it stays visible and
reconnection is disabled. Choose the new address in **Netzwerkdetails** and exchange
new codes. Changed ports also require updating the codes. A timeout cannot establish
whether the remote address changed: check the network and update codes if necessary.
New addresses for the same UUID keep the existing public-key pin; changed keys are
still rejected. No key rotation or automatic address replacement is performed.

`peer/transport` owns TCP framing and `Noise_XX_25519_ChaChaPoly_BLAKE2s` through
`snow`. Both static keys are checked against exchanged codes, then both peers send
an encrypted identity/version confirmation before messages are accepted. The
coordinator and delivery policy have no Tauri or SQL dependency; platform commands
provide database, secure-store and event adapters. `sqlite_peer` owns SQL operations.

Schema 4 adds transport identity, selected-peer settings, remembered public-key pins
and delivery records. Existing IDs, identity and messages survive migration. Previously
seen UUID/key pairs stay pinned even when another peer is selected. Changing a key
for an existing UUID is rejected; key rotation is outside this step.

An outgoing message and its pending delivery record commit together. **Lokal gespeichert
· unbestätigt** means only this device has stored it. **Beim Gegenüber gespeichert**
means the authenticated peer acknowledged successful storage; it does not mean read.
The receiver also acknowledges duplicates without changing receipt time or inserting
another message. ACKs must match an outgoing message addressed to that peer, are
persisted before UI notification, and cannot restore an expired message.

Each direction has one message awaiting an ACK. After ten seconds without an ACK,
retries wait 1, 2, 4, 8, 16, then 30 seconds, always preserving ID and content. During
an explicitly started session, network interruptions trigger reconnection with the
same backoff. Verbindung trennen and app exit stop attempts. App restart does not open any sockets
or reconnect: choose **Erneut verbinden** to resume still-valid pending deliveries. No new
peer messages are accepted offline. Retries stop at exactly 24 hours; cleanup deletes
delivery metadata with the message. SQLite reuses freed pages without periodic VACUUM.

Encrypted frames are bounded at 65,535 bytes, with at most 60 KiB of JSON including
metadata. Oversized peer messages fail before storage and preserve the draft; local
messages retain their existing text rules. A five-second connect timeout, ten-second
handshake/write timeout and idle heartbeat detect unreachable or stalled peers.
Authentication and protocol failures stop active retries until the user starts again.

The outbox is read on connection, enqueue, ACK and retry rather than polled while idle.
The frontend subscribes before reading snapshots, refreshes from SQLite after changes,
and ignores old load/state responses. Durable ACK status is never overwritten by a
late pending send response. Focus refreshes history and connection state; there is no
frontend polling. Separate connection components/store own setup and lifecycle;
message components/store own display and editable drafts.

Validation includes two real loopback coordinators with separate temporary databases,
lost ACKs, bidirectional delivery, reconnects, wrong keys/identities, malformed frames,
migrations, exact retention boundaries, and store races. Tests also cover read-only setup without secret-store calls, both saved roles after
full app-state recreation, resumed delivery with the original ID, migration rollback,
atomic profile writes, missing addresses, duplicate actions, stale snapshots and
clipboard failure. The compact reconnect UI was checked with isolated browser fixtures.
CI runs Rust/TypeScript tests and produces production packages on macOS and Windows.
The existing macOS two-app LAN acceptance covered bidirectional messages, drafts,
storage failures, retries and restart with outstanding delivery. Native Internet,
Windows and cross-platform two-router acceptance are still pending; automated
transport tests do not replace those device checks.

---

## Project Goals and Roadmap

### 1. UI and Widget Design (Frontend)

- [x] Implement an opaque, flat petrol-green dark UI with accessible tabs and separate connection setup
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

- [x] Implement one direct TCP peer connection on macOS and Windows (LAN)
- [x] Authenticate exchanged public keys and encrypt messages with Noise
- [x] Add Internet transport with a fixed relay and direct-path upgrades (pilot)
- [x] Refresh incoming messages and durable delivery status through Tauri events

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
