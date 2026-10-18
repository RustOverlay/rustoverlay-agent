//! A loopback relay and an uplink pointed at it.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use prost::Message as _;
use ro_core::hosting::GameEventBus;
use ro_net::enrollment::TokenStore;
use ro_net::proto::{self, server_frame};
use ro_net::relay::{ActiveTeam, ActiveTeamStore, RelayOptions, RelayUplink, TeamRoomState};
use ro_vision::hosting::LoadoutState;
use tempfile::TempDir;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use url::Url;

use super::{MemoryTokenStore, hexay};

pub const PATIENCE: Duration = Duration::from_secs(10);

pub fn squad(join_code: &str) -> ActiveTeam {
    ActiveTeam { team_id: "t".into(), join_code: join_code.into(), name: "Squad".into() }
}

pub fn server_frame(payload: server_frame::Payload) -> Message {
    Message::binary(proto::ServerFrame { payload: Some(payload) }.encode_to_vec())
}

pub struct Harness {
    pub listener: TcpListener,
    pub teams: ActiveTeamStore,
    pub tokens: Arc<MemoryTokenStore>,
    pub loadout: Arc<Mutex<LoadoutState>>,
    pub room: Arc<TeamRoomState>,
    pub bus: GameEventBus,
    running: JoinHandle<()>,
    _dir: TempDir,
}

impl Harness {
    /// Signed in and active on 7926ZG. `options.base_url` is replaced with the loopback relay's.
    pub async fn start(options: RelayOptions) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let dir = tempfile::tempdir().unwrap();
        let teams = ActiveTeamStore::new(dir.path(), "active-team.json");
        teams.save(Some(&squad("7926ZG"))).unwrap();
        let tokens = Arc::new(MemoryTokenStore::default());
        tokens.save(&hexay()).unwrap();

        let loadout = Arc::new(Mutex::new(LoadoutState::new()));
        let room = Arc::new(TeamRoomState::new());
        let bus = GameEventBus::new();
        let options = RelayOptions { base_url: Url::parse(&format!("ws://127.0.0.1:{port}")).unwrap(), ..options };
        let uplink = RelayUplink::new(teams.clone(), tokens.clone(), loadout.clone(), room.clone(), bus.clone(), options);
        let running = tokio::spawn(async move { uplink.run().await });

        Self { listener, teams, tokens, loadout, room, bus, running, _dir: dir }
    }

    pub async fn accept(&self) -> WebSocketStream<TcpStream> {
        accept(&self.listener).await
    }

    /// Whether the uplink dials in within `window`. A connection that does arrive is discarded.
    pub async fn connects_within(&self, window: Duration) -> bool {
        tokio::time::timeout(window, self.listener.accept()).await.is_ok()
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.running.abort();
    }
}

/// The relay side of one WebSocket dialled into `listener`.
pub async fn accept(listener: &TcpListener) -> WebSocketStream<TcpStream> {
    let (tcp, _) = tokio::time::timeout(PATIENCE, listener.accept()).await.unwrap().unwrap();
    tokio_tungstenite::accept_async(tcp).await.unwrap()
}

pub async fn next(socket: &mut WebSocketStream<TcpStream>) -> proto::ClientFrame {
    loop {
        if let Message::Binary(bytes) = tokio::time::timeout(PATIENCE, socket.next()).await.unwrap().unwrap().unwrap() {
            return proto::ClientFrame::decode(bytes).unwrap();
        }
    }
}

/// Reads past whatever the uplink still sends until it hangs up.
pub async fn closed(socket: &mut WebSocketStream<TcpStream>) {
    loop {
        match tokio::time::timeout(PATIENCE, socket.next()).await.expect("the uplink kept the socket open") {
            None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return,
            Some(Ok(_)) => {}
        }
    }
}

pub async fn until(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(PATIENCE, async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("condition never held");
}
