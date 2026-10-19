# Testing and acceptance

[Overview](../README.md) · [Architecture](architecture.md) · [Security](security.md) · [Connections](connections.md)

## Commands and CI

Run the [README checks and builds](../README.md#build-and-check) from the repository
root after `npm ci`. Node.js 22.6+ loads TypeScript with type stripping; Svelte-Check
provides the type check. `npm test` finds `*.test.mjs` one level below `tests/frontend/`
(currently `controllers/` and `stores/`).

[Desktop CI](../.github/workflows/check.yml) runs frontend/Rust checks, SQLCipher
source verification and production packages on `macos-latest` and `windows-latest`.
Inspect the actual run for the revision being reviewed; workflow configuration alone
is not a passing result. Native prerequisites: [README](../README.md#get-started)
and [SQLCipher build notes](../src-tauri/vendor/README.md).

## Automated coverage

| Area | Focus |
| --- | --- |
| Frontend | Drafts, tabs/modes, validation, stale responses, focus refresh, clipboard, retry and listener cleanup |
| Messages/storage | Validation, ordering, exact retention, duplicates/conflicts, concurrent writes and durable ACKs |
| Migrations | Legacy data/pin preservation, incomplete setup, invalid schemas/profiles and rollback |
| Encryption | Wrong/missing keys, interrupted conversion/cleanup and WAL/journal plaintext canaries |
| LAN/shared sessions | Loopback delivery, lost ACKs, retries, reconnect, restart with pending messages and shutdown |
| Internet | Local relays, direct Noise streams, forced-relay delivery, wrong identities, outage, stream limits and canceled setup/handshake |

Tests use temporary databases, injected credentials and controlled clocks/promises;
they do not access personal history or OS keys. Internet tests use local relays,
not the public service. Tauri command tests use the mock runtime where applicable.

## Native acceptance

Earlier checks: two isolated macOS LAN apps, a forced-public-relay macOS Noise probe,
and browser fixtures for compact UI/keyboard behavior. These do not certify the
current revision or cross-platform operation. Device checks still pending:

- [ ] Windows build/runtime and macOS/Windows credential prompts.
- [ ] macOS ↔ Windows behind different routers; simultaneous/staggered starts and bidirectional messages/ACKs.
- [ ] Direct/forced-relay operation with UDP blocked; accurate path changes without duplicate delivery or reauthentication.
- [ ] Relay loss, standby, network changes and subsequent reconnect.
- [ ] Restart with pending delivery: preserve identity, history, message ID and delivery state; retain drafts across view changes.
- [ ] Repeated connect/disconnect and cancellation during setup/handshake; disconnect stops all connection activity.
- [ ] Both platforms at 350 × 500 and enlarged: opaque/shadow-free window, long content, scroll areas, keyboard/focus, window actions and resize.

Use isolated app data/test identities for failure exercises; record results against
the tested revision.
