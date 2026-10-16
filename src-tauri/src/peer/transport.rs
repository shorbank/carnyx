//! Framing and authenticated encryption over an ordered bidirectional stream. No database or Tauri dependency.
use super::{
    PeerError, Result,
    keys::Keys,
    protocol::{Frame, MAX_FRAME, NOISE, PROLOGUE},
    types::PeerIdentity,
};
use crate::identity::PeerId;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    time::{Duration, timeout},
};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

pub trait DuplexStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> DuplexStream for T {}
pub type Stream = Box<dyn DuplexStream>;
type Socket = Framed<Stream, LengthDelimitedCodec>;
pub struct Session {
    socket: Socket,
    noise: snow::TransportState,
    pub peer: PeerId,
}
fn crypto(e: snow::Error) -> PeerError {
    PeerError::new("authentication_failed", e)
}
fn framing(e: std::io::Error) -> PeerError {
    if e.kind() == std::io::ErrorKind::InvalidData {
        PeerError::new("protocol_error", e)
    } else {
        e.into()
    }
}
async fn read(socket: &mut Socket) -> Result<Vec<u8>> {
    socket
        .next()
        .await
        .ok_or_else(|| PeerError::new("network_error", "peer closed connection"))?
        .map(|b| b.to_vec())
        .map_err(framing)
}
async fn write(socket: &mut Socket, bytes: Vec<u8>) -> Result<()> {
    timeout(Duration::from_secs(10), socket.send(Bytes::from(bytes)))
        .await
        .map_err(|_| PeerError::new("timeout", "write timeout"))?
        .map_err(framing)
}
pub async fn handshake<S: DuplexStream + 'static>(
    stream: S,
    keys: &Keys,
    own: PeerId,
    expected: &PeerIdentity,
    initiator: bool,
) -> Result<Session> {
    timeout(Duration::from_secs(10), async {
        let mut socket = Framed::new(
            Box::new(stream) as Stream,
            LengthDelimitedCodec::builder()
                .max_frame_length(MAX_FRAME)
                .new_codec(),
        );
        let builder = snow::Builder::new(NOISE.parse().expect("valid Noise pattern"))
            .prologue(PROLOGUE)
            .map_err(crypto)?
            .local_private_key(&keys.private)
            .map_err(crypto)?;
        let mut state = if initiator {
            builder.build_initiator()
        } else {
            builder.build_responder()
        }
        .map_err(crypto)?;
        let mut buffer = vec![0; MAX_FRAME];
        for turn in 0..3 {
            if (turn % 2 == 0) == initiator {
                let size = state.write_message(&[], &mut buffer).map_err(crypto)?;
                write(&mut socket, buffer[..size].to_vec()).await?;
            } else {
                let bytes = read(&mut socket).await?;
                state.read_message(&bytes, &mut buffer).map_err(crypto)?;
                if let Some(remote) = state.get_remote_static() {
                    if remote != expected.public_key {
                        return Err(PeerError::new(
                            "authentication_failed",
                            "remote key differs from connection code",
                        ));
                    }
                }
            }
        }
        if state.get_remote_static() != Some(expected.public_key.as_slice()) {
            return Err(PeerError::new(
                "authentication_failed",
                "remote key missing or mismatched",
            ));
        }
        let mut session = Session {
            socket,
            noise: state.into_transport_mode().map_err(crypto)?,
            peer: expected.peer,
        };
        session
            .send(&Frame::Ready {
                version: 1,
                peer_id: own.to_string(),
                recipient_id: expected.peer.to_string(),
            })
            .await?;
        match session.receive().await? {
            Frame::Ready {
                version: 1,
                peer_id,
                recipient_id,
            } if peer_id == expected.peer.to_string() && recipient_id == own.to_string() => {
                Ok(session)
            }
            _ => Err(PeerError::new(
                "authentication_failed",
                "invalid peer identity or protocol version",
            )),
        }
    })
    .await
    .map_err(|_| PeerError::new("timeout", "handshake timeout"))?
}
impl Session {
    pub async fn send(&mut self, frame: &Frame) -> Result<()> {
        let bytes = frame.encode()?;
        let mut ciphertext = vec![0; bytes.len() + 16];
        let size = self
            .noise
            .write_message(&bytes, &mut ciphertext)
            .map_err(crypto)?;
        ciphertext.truncate(size);
        write(&mut self.socket, ciphertext).await
    }
    pub async fn receive(&mut self) -> Result<Frame> {
        let ciphertext = read(&mut self.socket).await?;
        let mut plaintext = vec![0; ciphertext.len()];
        let size = self
            .noise
            .read_message(&ciphertext, &mut plaintext)
            .map_err(crypto)?;
        Frame::decode(&plaintext[..size])
    }
    pub async fn close(mut self) {
        let _ = timeout(Duration::from_secs(1), async {
            self.send(&Frame::Close).await?;
            SinkExt::<Bytes>::close(&mut self.socket)
                .await
                .map_err(framing)
        })
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::Invitation;
    use super::*;
    use tokio::{
        io::AsyncWriteExt,
        net::{TcpListener, TcpStream},
    };
    fn key() -> Keys {
        let key = snow::Builder::new(NOISE.parse().unwrap())
            .generate_keypair()
            .unwrap();
        Keys {
            private: zeroize::Zeroizing::new(key.private),
            public: key.public.try_into().unwrap(),
        }
    }
    async fn pair() -> (Session, Session) {
        let a = key();
        let b = key();
        let aid = PeerId::generate();
        let bid = PeerId::generate();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = match listener.local_addr().unwrap() {
            std::net::SocketAddr::V4(a) => a,
            _ => unreachable!(),
        };
        let ai = Invitation::new(aid, &a.public, address);
        let bi = Invitation::new(bid, &b.public, address);
        let (a, b) = tokio::join!(
            async {
                handshake(
                    TcpStream::connect(address).await.unwrap(),
                    &a,
                    aid,
                    &bi.identity().unwrap(),
                    true,
                )
                .await
            },
            async {
                let (stream, _) = listener.accept().await.unwrap();
                handshake(stream, &b, bid, &ai.identity().unwrap(), false).await
            }
        );
        (a.unwrap(), b.unwrap())
    }
    #[tokio::test]
    async fn altered_ciphertext_and_oversized_prefix_are_rejected() {
        let (mut a, mut b) = pair().await;
        let payload = Frame::Ping { id: 1 }.encode().unwrap();
        let mut bytes = vec![0; payload.len() + 16];
        a.noise.write_message(&payload, &mut bytes).unwrap();
        bytes[0] ^= 1;
        a.socket.send(Bytes::from(bytes)).await.unwrap();
        assert_eq!(b.receive().await.unwrap_err().code, "authentication_failed");
        let (mut a, mut b) = pair().await;
        a.socket
            .get_mut()
            .write_all(&((MAX_FRAME + 1) as u32).to_be_bytes())
            .await
            .unwrap();
        assert_eq!(b.receive().await.unwrap_err().code, "protocol_error");
    }
    #[tokio::test]
    async fn fragmented_and_truncated_frames_are_handled_without_losing_boundaries() {
        let (mut a, mut b) = pair().await;
        let payload = Frame::Ping { id: 77 }.encode().unwrap();
        let mut bytes = vec![0; payload.len() + 16];
        a.noise.write_message(&payload, &mut bytes).unwrap();
        let wire = [
            (bytes.len() as u32).to_be_bytes().as_slice(),
            bytes.as_slice(),
        ]
        .concat();
        let (write, read) = tokio::join!(
            async {
                for part in wire.chunks(3) {
                    a.socket.get_mut().write_all(part).await.unwrap();
                    tokio::task::yield_now().await;
                }
            },
            b.receive()
        );
        let _ = write;
        assert!(matches!(read.unwrap(), Frame::Ping { id: 77 }));
        // Queue a complete encrypted frame while the next receive is cancelled.
        let _ = timeout(Duration::from_millis(1), b.receive()).await;
        a.send(&Frame::Ping { id: 78 }).await.unwrap();
        assert!(matches!(b.receive().await.unwrap(), Frame::Ping { id: 78 }));
        a.socket
            .get_mut()
            .write_all(&100u32.to_be_bytes())
            .await
            .unwrap();
        a.socket.get_mut().write_all(&[1, 2]).await.unwrap();
        a.socket.get_mut().shutdown().await.unwrap();
        assert!(b.receive().await.is_err());
    }
    #[test]
    fn malformed_plaintext_and_unknown_protocol_messages_are_rejected() {
        assert_eq!(
            Frame::decode(br#"{"type":"unknown"}"#).unwrap_err().code,
            "protocol_error"
        );
        assert!(Frame::decode(b"not JSON").is_err());
        assert!(Frame::decode(&vec![b' '; super::super::protocol::MAX_JSON + 1]).is_err());
    }
}
