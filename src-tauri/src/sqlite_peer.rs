//! SQL for peer identity and delivery records; the transport never sees a connection.
use crate::{
    identity::PeerId,
    peer::{
        PeerError, Result,
        delivery::{DeliveredMessage, DeliveryRepository, PendingMessage},
        keys::KeyRepository,
        types::{Invitation, Role, SessionSettings},
    },
    sqlite_repository::{SqliteMessageRepository, read_message},
};
use rusqlite::{OptionalExtension, params};

fn sql(e: rusqlite::Error) -> PeerError {
    PeerError::new("storage_error", e)
}
impl KeyRepository for SqliteMessageRepository {
    fn public_key(&self) -> Result<Option<[u8; 32]>> {
        let value: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT public_key FROM transport_identity WHERE slot=1",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        value
            .map(|v| {
                v.try_into()
                    .map_err(|_| PeerError::new("key_unavailable", "invalid public key"))
            })
            .transpose()
    }
    fn save_public_key(&mut self, key: &[u8; 32]) -> Result<()> {
        self.connection
            .execute(
                "INSERT INTO transport_identity(slot,public_key) VALUES(1,?1)",
                params![key.as_slice()],
            )
            .map_err(sql)?;
        Ok(())
    }
    fn selected_peer(&self) -> Result<Option<Invitation>> {
        let value: Option<(String, Vec<u8>, String)> = self
            .connection
            .query_row(
                "SELECT peer_id,public_key,address FROM trusted_peer WHERE slot=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(sql)?;
        value
            .map(|(peer, key, address)| {
                let peer = peer
                    .parse()
                    .map_err(|e| PeerError::new("storage_error", e))?;
                let key = key
                    .try_into()
                    .map_err(|_| PeerError::new("storage_error", "invalid trusted key"))?;
                let address = address
                    .parse()
                    .map_err(|e| PeerError::new("storage_error", e))?;
                let invitation = Invitation::new(peer, &key, address);
                Invitation::decode(&invitation.encode())
            })
            .transpose()
    }
    #[cfg(test)]
    fn select_peer(&mut self, peer: &Invitation) -> Result<()> {
        self.persist_peer(peer, None)
    }
}

impl SqliteMessageRepository {
    pub(crate) fn saved_session(&self, peer: PeerId) -> Result<Option<SessionSettings>> {
        let value: Option<(String, u16, String)> = self.connection.query_row(
            "SELECT local_address,local_port,role FROM peer_session_settings WHERE slot=1 AND peer_id=?1",
            params![peer.to_string()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).optional().map_err(sql)?;
        value
            .map(|(address, port, role)| {
                let role = match role.as_str() {
                    "initiator" => Role::Initiator,
                    "responder" => Role::Responder,
                    _ => return Err(PeerError::new("storage_error", "invalid saved role")),
                };
                let settings = SessionSettings {
                    local_address: address
                        .parse()
                        .map_err(|e| PeerError::new("storage_error", e))?,
                    local_port: port,
                    role,
                };
                settings
                    .endpoint()
                    .map_err(|e| PeerError::new("storage_error", e))?;
                Ok(settings)
            })
            .transpose()
    }
    pub(crate) fn select_session(
        &mut self,
        peer: &Invitation,
        settings: &SessionSettings,
    ) -> Result<()> {
        settings.endpoint()?;
        self.persist_peer(peer, Some(settings))
    }
    fn persist_peer(
        &mut self,
        peer: &Invitation,
        settings: Option<&SessionSettings>,
    ) -> Result<()> {
        // Validate the same invitation path for initial setup and stored reconnects.
        Invitation::decode(&peer.encode())?;
        let transaction = self.connection.transaction().map_err(sql)?;
        let existing: Option<Vec<u8>> = transaction
            .query_row(
                "SELECT public_key FROM peer_keys WHERE peer_id=?1",
                params![peer.peer_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        let key = peer.key()?;
        crate::peer::keys::validate_pin(existing.as_deref(), &key)?;
        transaction.execute("INSERT INTO peer_keys(peer_id,public_key) VALUES(?1,?2) ON CONFLICT(peer_id) DO NOTHING", params![peer.peer_id,key.as_slice()]).map_err(sql)?;
        transaction.execute("INSERT INTO trusted_peer(slot,peer_id,public_key,address) VALUES(1,?1,?2,?3) ON CONFLICT(slot) DO UPDATE SET peer_id=excluded.peer_id,public_key=excluded.public_key,address=excluded.address",params![peer.peer_id,key.as_slice(),peer.address.to_string()]).map_err(sql)?;
        if let Some(settings) = settings {
            let role = match settings.role {
                Role::Initiator => "initiator",
                Role::Responder => "responder",
            };
            transaction.execute("INSERT INTO peer_session_settings(slot,peer_id,local_address,local_port,role) VALUES(1,?1,?2,?3,?4) ON CONFLICT(slot) DO UPDATE SET peer_id=excluded.peer_id,local_address=excluded.local_address,local_port=excluded.local_port,role=excluded.role",
                params![peer.peer_id, settings.local_address.to_string(), settings.local_port, role]).map_err(sql)?;
        } else {
            transaction
                .execute("DELETE FROM peer_session_settings", [])
                .map_err(sql)?;
        }
        transaction.commit().map_err(sql)?;
        Ok(())
    }
}

const FIELDS: &str =
    "m.id,m.text,m.created_at,m.sender_id,m.recipient_id,m.direction,m.received_at";
impl DeliveryRepository for SqliteMessageRepository {
    fn pending(&self, peer: PeerId, cutoff: i64) -> Result<Option<PendingMessage>> {
        self.connection.query_row(&format!("SELECT {FIELDS},d.attempts FROM messages m JOIN deliveries d ON d.message_id=m.id WHERE m.recipient_id=?1 AND m.direction='outgoing' AND m.created_at>?2 AND d.state='pending' ORDER BY m.created_at,m.rowid LIMIT 1"),params![peer.to_string(),cutoff],|r|Ok(PendingMessage {message:read_message(r)?,attempts:r.get(7)?})).optional().map_err(sql)
    }
    fn record_attempt(&mut self, id: &str, cutoff: i64) -> Result<bool> {
        Ok(self.connection.execute("UPDATE deliveries SET attempts=MIN(attempts+1,4294967295) WHERE message_id=?1 AND state='pending' AND EXISTS(SELECT 1 FROM messages m WHERE m.id=message_id AND m.created_at>?2)",params![id,cutoff]).map_err(sql)?==1)
    }
    fn acknowledge(&mut self, id: &str, peer: PeerId, cutoff: i64, now: i64) -> Result<bool> {
        Ok(self.connection.execute("UPDATE deliveries SET state='acknowledged',acknowledged_at=?1 WHERE message_id=?2 AND state='pending' AND EXISTS(SELECT 1 FROM messages m WHERE m.id=message_id AND m.direction='outgoing' AND m.recipient_id=?3 AND m.created_at>?4)",params![now,id,peer.to_string(),cutoff]).map_err(sql)?==1)
    }
    fn peer_history(&self, peer: PeerId, cutoff: i64) -> Result<Vec<DeliveredMessage>> {
        let mut query=self.connection.prepare(&format!("SELECT {FIELDS},d.acknowledged_at FROM messages m LEFT JOIN deliveries d ON d.message_id=m.id WHERE ((m.direction='outgoing' AND m.recipient_id=?1) OR (m.direction='incoming' AND m.sender_id=?1)) AND COALESCE(m.received_at,m.created_at)>?2 ORDER BY COALESCE(m.received_at,m.created_at),m.rowid")).map_err(sql)?;
        query
            .query_map(params![peer.to_string(), cutoff], |r| {
                Ok(DeliveredMessage {
                    message: read_message(r)?,
                    acknowledged_at: r.get(7)?,
                })
            })
            .map_err(sql)?
            .collect::<std::result::Result<_, _>>()
            .map_err(sql)
    }
}
