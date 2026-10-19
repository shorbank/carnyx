# Security

[Overview](../README.md) · [Architecture](architecture.md) · [Connections](connections.md) · [Testing](testing.md)

## Protection and trust

| Area | Behavior |
| --- | --- |
| Peer traffic | LAN and Internet share `Noise_XX_25519_ChaChaPoly_BLAKE2s`; keys and encrypted identity/version confirmations must match before messaging |
| Trust | Exchange public codes through a trusted channel; known UUID/key pairs stay pinned, even after selecting another peer |
| Internet transport | Separate iroh identity; only the selected transport peer and configured HTTPS relay are accepted |
| Local history | SQLCipher 4.19.0 encrypts the full database with AES-256 page protection/authentication, including profiles, pins and deliveries |
| Temporary storage | Journal/WAL page payloads are encrypted; temporary SQL storage stays in memory |
| Relay privacy | The operator can observe transport IDs, IPs and traffic metadata; availability is not guaranteed |

A UUID alone does not authenticate a device. Changed pinned keys are rejected.
There is no public address publication, automatic port mapping, relay selection or failover.

## Independent keys

Secrets live in macOS Keychain or Windows Credential Manager. The database stores
public transport keys; the random 32-byte history key encrypts the database itself.

| Purpose | Credential service | Account |
| --- | --- | --- |
| History | `<app identifier>.history-key.v1` | `messages.sqlite` |
| Noise | `<app identifier>.noise-key.v1` | Local Carnyx UUID |
| Internet | `<app identifier>.internet-key.v1` | Local Carnyx UUID |

- Keys stay in Rust with zeroizing buffers; SQLCipher receives its key via `sqlite3_key`, never SQL text or IPC.
- OS calls run outside database locks. Missing/mismatched existing keys are never silently replaced.
- Interrupted first setup can reuse an already stored secret. Internet-key failure leaves LAN/local chat available.

## Recovery

Legacy plaintext conversion follows **copy → reopen and verify → retire source →
promote encrypted copy → cleanup**. Checks include rows, schema, rowids, integrity
and a rolled-back schema upgrade. Progress markers allow interrupted phases to resume;
conversion needs space for another full database.

Ambiguous artifacts, missing/wrong keys, corrupt data or failed cleanup block startup
with retry. No automatic reset or plaintext fallback is provided. Retention starts
after conversion/cleanup. Implementation: [encrypted_storage/migration.rs](../src-tauri/src/encrypted_storage/migration.rs).

## Limits and build

- Encryption does not protect against malware acting as the unlocked OS user or plaintext already in app memory.
- Losing the OS key makes history unreadable; a database-only copy is insufficient. Key backup, rotation and password unlocking are not implemented.
- Removing old plaintext files cannot securely erase SSD pages, snapshots or backups.
- macOS uses CommonCrypto; Windows uses static OpenSSL. Source hashes and cipher version are verified.

See [SQLCipher sources, build requirements and licenses](../src-tauri/vendor/README.md)
and [pending native checks](testing.md#native-acceptance).
