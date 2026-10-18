use std::time::Duration;

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use prost::Message as _;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Mutex;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tokio_tungstenite::tungstenite::{self, Message};

use crate::proto::{ClientFrame, ServerFrame};

pub(crate) const MAX_FRAME_BYTES: usize = 64 * 1024;

/// Why a relay session ended other than a clean close.
#[derive(Debug)]
pub enum RelayError {
    WebSocket(tungstenite::Error),
    Protocol(String),
    Decode(prost::DecodeError),
}

impl std::fmt::Display for RelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WebSocket(e) => write!(f, "{e}"),
            Self::Protocol(message) => f.write_str(message),
            Self::Decode(e) => write!(f, "undecodable server frame: {e}"),
        }
    }
}

impl std::error::Error for RelayError {}

impl From<tungstenite::Error> for RelayError {
    fn from(e: tungstenite::Error) -> Self {
        match e {
            tungstenite::Error::Capacity(_) => Self::Protocol("frame exceeds 64 KiB".into()),
            other => Self::WebSocket(other),
        }
    }
}

/// The 64 KiB inbound cap the C# receive buffer enforced.
pub(crate) fn socket_config() -> WebSocketConfig {
    WebSocketConfig::default().max_message_size(Some(MAX_FRAME_BYTES)).max_frame_size(Some(MAX_FRAME_BYTES))
}

/// One WebSocket session with a relay room: join, then binary protobuf frames both ways.
///
/// Deliberately dumb — it owns the socket and the framing, not the reconnect policy or what to
/// send. [`super::RelayUplink`] owns those, so this stays testable against a loopback socket.
pub struct RelayConnection<S> {
    // One writer at a time; the heartbeat races the uplink.
    sink: Mutex<SplitSink<WebSocketStream<S>, Message>>,
    stream: Mutex<SplitStream<WebSocketStream<S>>>,
}

impl<S: AsyncRead + AsyncWrite + Unpin> RelayConnection<S> {
    pub fn new(socket: WebSocketStream<S>) -> Self {
        let (sink, stream) = socket.split();
        Self { sink: Mutex::new(sink), stream: Mutex::new(stream) }
    }

    pub async fn send(&self, frame: &ClientFrame) -> Result<(), RelayError> {
        self.sink.lock().await.send(Message::binary(frame.encode_to_vec())).await?;
        Ok(())
    }

    /// None when the peer closed cleanly. `idle` bounds the wait for any frame, pings included.
    pub async fn receive(&self, idle: Option<Duration>) -> Result<Option<ServerFrame>, RelayError> {
        let mut stream = self.stream.lock().await;
        loop {
            let next = match idle {
                Some(idle) => tokio::time::timeout(idle, stream.next())
                    .await
                    .map_err(|_| RelayError::Protocol(format!("no frame from the relay in {}s", idle.as_secs_f32())))?,
                None => stream.next().await,
            };
            let payload = match next {
                None | Some(Ok(Message::Close(_))) => return Ok(None),
                Some(Err(e)) => return Err(e.into()),
                Some(Ok(Message::Binary(bytes))) => bytes,
                // The C# socket parsed a text message's bytes the same way.
                Some(Ok(Message::Text(text))) => text.as_bytes().to_vec().into(),
                Some(Ok(_)) => continue,
            };
            return ServerFrame::decode(payload).map(Some).map_err(RelayError::Decode);
        }
    }

    /// Best-effort: the relay times the member out regardless.
    pub async fn close(&self) {
        let frame = CloseFrame { code: CloseCode::Normal, reason: "".into() };
        let mut sink = self.sink.lock().await;
        if sink.send(Message::Close(Some(frame))).await.is_ok() {
            let _ = sink.close().await;
        }
    }
}
