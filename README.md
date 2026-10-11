# Carnyx

A lightweight, transparent desktop overlay chat application built for direct peer-to-peer (P2P) communication without relying on a central message-storage server

---

## Tech Stack

- Desktop Framework: [Tauri v2](https://tauri.app/)
- Frontend: [SvelteKit](https://svelte.dev/) + [TypeScript](https://www.typescriptlang.org/) + Vite
- Styling: Tailwind CSS
- Backend / Storage: Rust

---

## Project Goals and Roadmap

### 1. UI and Widget Design (Frontend)

- [ ] Implement a semi-transparent glassmorphism chat UI
- [ ] Configure window click-through behavior (interact only with the chat window, pass clicks through the transparent background to the desktop)
- [ ] Add message rendering and auto-scroll behavior

### 2. Backend and 24-Hour Cache (Rust)

- [ ] Set up a local embedded database (`rusqlite`) in the application data directory
- [ ] Implement a database schema for messages (`id`, `sender`, `recipient`, `text`, `timestamp`)
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
