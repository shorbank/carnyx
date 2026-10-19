# Carnyx

A lightweight desktop overlay chat built with Tauri v2, SvelteKit, TypeScript and
Rust. Carnyx supports a local conversation and one authenticated peer over LAN or
Internet, without a central message-storage server.

## Features and scope

- End-to-end encrypted peer messages with durable delivery acknowledgements.
- Local history encrypted with SQLCipher and retained for 24 hours.
- Explicit code exchange, saved connection settings and reconnect.
- Opaque petrol-green UI with separate **Lokal**, **Peer** and **Verbindung** tabs.
- One resizable, always-on-top desktop window; duplicate launches focus it.

The desktop targets are macOS and Windows. Internet connectivity uses a fixed public
EU relay as a pilot dependency and can upgrade to a direct path. Both peers must
start a session; accounts, offline delivery, multiple peers and click-through are
not implemented. See [connection setup and limits](docs/connections.md).

## Get started

Requirements: Node.js 22.6+, current stable Rust (iroh requires 1.91+), and Python 3
for source verification. Native builds need Xcode Command Line Tools on macOS;
MSVC C++ tools, WebView2 and Perl on Windows (bundled OpenSSL).

```sh
npm ci
npm run tauri dev
```

`npm run dev` starts only the frontend; storage/chat need the Tauri runtime.

## Build and check

```sh
npm test
npm run check
npm run build
python scripts/verify-sqlcipher.py
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo check --manifest-path src-tauri/Cargo.toml --locked
cargo test --manifest-path src-tauri/Cargo.toml --lib --locked
```

`npm run build` builds the frontend. To produce a native desktop package, run:

```sh
# macOS application bundle
npm run tauri build -- --bundles app

# Windows installer (run on Windows)
npm run tauri build -- --bundles nsis
```

## Documentation

| Document | Contents |
| --- | --- |
| [Architecture](docs/architecture.md) | File layout, responsibilities, data flow and schema migrations |
| [Security](docs/security.md) | Encryption, key storage, trust and recovery limits |
| [Connections](docs/connections.md) | Internet/LAN setup, reconnect, delivery and troubleshooting |
| [Testing](docs/testing.md) | Commands, CI, automated coverage and pending native acceptance |
| [SQLCipher build](src-tauri/vendor/README.md) | Pinned sources, reproduction and licenses |
