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

The overlay currently supports a single local conversation stored only in memory.
Enter sends a message; Shift+Enter inserts a line break. The Send button also sends
and returns focus to the input. Messages display their local time. The list follows
new messages when already at the bottom and preserves the reading position when
scrolled up. Only the header moves the window.

Reloading or closing the application clears all UI messages. The UI is not yet
connected to the Rust database. P2P communication and click-through behavior
are not implemented in this milestone.
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

## Rust Message Storage Foundation

Startup opens `messages.sqlite` in Tauri's application data directory and keeps a
message service in managed application state. On first use, a transaction creates
schema version 1: `messages(id, text, created_at)`. Unknown schema versions and
initialization errors stop startup; existing databases are never reset.

The modules in `src-tauri/src/` have separate responsibilities:

- `message`: trims outer whitespace, rejects empty text, and creates UUID-v4 IDs
  and Unix timestamps in milliseconds. Inner whitespace and Unicode are preserved.
- `message_service`: creates and stores a message through a repository interface,
  returning success only after storage succeeds.
- `sqlite_repository`: owns the SQLite connection, schema initialization, parameterized
  inserts, and lookup by ID. SQLite is bundled through `rusqlite`.

There are no new Tauri commands or frontend changes yet, so messages entered in
the current chat still live only in memory. The Rust database currently has no
24-hour cleanup, expiry filtering, or scheduled tasks. Sender and recipient fields
will be introduced when those concepts are needed.

Rust module tests use temporary directories, including reopening a database,
duplicate IDs, validation, and failure cases. Run from `src-tauri`:

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
- [ ] Add sender and recipient fields when peer identities are introduced
- [ ] Write an automated cleanup task to purge messages older than 24 hours on application startup
- [ ] Expose Tauri Commands for the frontend to send and receive messages

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
