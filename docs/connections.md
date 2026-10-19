# Connections

[Overview](../README.md) · [Architecture](architecture.md) · [Security](security.md) · [Testing](testing.md)

## Connect two devices

Both devices must explicitly start a session. Exchange codes through a trusted
channel; use **Code kopieren** or select/copy the text manually.

| Step | Internet (pilot) | Local network (LAN) |
| --- | --- | --- |
| 1 | Open **Verbindung → Internet** on both devices | Open **Verbindung → Lokales Netzwerk** on both devices |
| 2 | Choose **Eigenen Code anzeigen** | Select local IPv4/port under **Netzwerkdetails** (default `32147`), then show the code |
| 3 | Exchange codes and paste the other's code | Exchange codes matching the selected addresses/ports |
| 4 | Click **Verbinden** on both devices; roles are automatic | One chooses **Auf Peer warten**, the other **Verbinden**; firewall access must be allowed |
| 5 | Open **Peer** after **Verschlüsselt verbunden** | Open **Peer** after **Verschlüsselt verbunden** |

Fresh setups default to Internet; saved setups retain their mode. Codes contain
public identities and connection data, never private keys. LAN uses `carnyx:v1`,
Internet `carnyx:v2`; Rust validates both with a 2048-character limit. A mode mismatch
is rejected without switching modes or falling back to another transport.

## Internet relay

iroh 1.3.0 uses the fixed EU relay `https://euc1-1.relay.n0.iroh.link./`, configured
in [peer/types.rs](../src-tauri/src/peer/types.rs). The relay is a pilot dependency
without guaranteed availability and can process [connection metadata](security.md).
Only this configured relay is accepted; changing it requires new codes.

Code generation needs no network connection or listener. Connect/reconnect opens the
endpoint. iroh can upgrade to a direct path without restarting Noise/delivery;
**Netzwerkdetails** shows the actual direct/relay path, or that it is being determined.

## Reconnect and troubleshooting

| Situation | Behavior / action |
| --- | --- |
| App restart | History/settings return; both peers must click **Erneut verbinden**. No automatic session starts |
| Network interruption | An active session retries; pending deliveries resume within retention |
| Edit setup | **Einrichtung ändern**; LAN edits and per-mode code drafts survive view/mode changes |
| Changed LAN address/port | Select the current endpoint and exchange new codes; unavailable saved addresses disable reconnect |
| Missing legacy LAN settings | Complete setup once; identity/history remain |
| Authentication failure | Check trusted codes; changed pinned keys are rejected |
| **Verbindung trennen** / exit | Stops endpoints, listeners, observers and retries |

Tab changes retain drafts and an active session. Refreshing/opening a tab never
connects. Endpoint/mode changes invalidate own codes and stale generation responses.

## Delivery and limits

- **Beim Gegenüber gespeichert** confirms durable storage, not reading. Unconfirmed means only the sender has stored it.
- Duplicates do not add rows or extend retention. ACKs must match the authenticated peer and cannot revive expired messages.
- One outstanding message per direction; ACK timeout **10 s**, retry/reconnect backoff **1, 2, 4, 8, 16, 30 s**. IDs/content stay unchanged.
- History/pending deliveries expire after **24 h**. No new offline sending, accounts, multiple peers or server-side queue.
- Maximum JSON including metadata: **60 KiB**; encrypted frame: **65,535 bytes**. Oversized sends preserve drafts.
- Connect timeout: **LAN 5 s / Internet 30 s**; Noise/write timeout **10 s**. Heartbeat after **15 s** idle, reply limit **10 s**.
- Waiting remains active until disconnected; untrusted Internet handshakes/stream setup have individual limits. Active-session authentication/protocol errors stop retries; LAN listeners reject failed incoming handshakes and keep waiting.
