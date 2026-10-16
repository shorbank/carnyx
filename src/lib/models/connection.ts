export type ConnectionMode = "lan" | "internet";
export type ConnectionTarget = { mode: "lan"; address: string } | { mode: "internet"; endpointId: string; relayUrl: string };
export interface Invitation { peerId: string; publicKey: string; target: ConnectionTarget }
export type PeerRole = "initiator" | "responder";
export type SessionSettings = { mode: "lan"; localAddress: string; localPort: number; role: PeerRole } | { mode: "internet" };
export interface PeerSetup { peerId: string; publicKey: string | null; selectedPeer: Invitation | null; session: SessionSettings | null; addresses: string[] }
export interface PeerState { phase: "disconnected" | "listening" | "connecting" | "handshaking" | "connected" | "reconnecting" | "disconnecting"; peerId: string | null; revision: number; error: string | null; mode: ConnectionMode | null; path: "direct" | "relay" | "unknown" | null }
