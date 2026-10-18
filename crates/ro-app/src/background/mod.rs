//! The long-running services the C# host registered as hosted services: the log tail, the relay
//! uplink, the UI event pump, the capture loop, the gunshot locator and the update check. Started only once sign-in has
//! passed — there is no signed-out mode, so capture and tailing have nothing to do without an
//! account.

mod event_pump;
mod gunshots;
mod log_tail;
mod skin_icons;
mod updates;

use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ro_capture::{StopToken, VisionService};
use ro_core::hosting::TailState;
use ro_net::relay::RelayUplink;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

use crate::host::Host;
use crate::state::Notify;
use crate::state::feed::EventFeed;

/// How long shutdown waits for the capture thread, as the C# host's stop timeout did.
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

pub struct Services {
    tasks: Vec<JoinHandle<()>>,
    vision: Option<(StopToken, Receiver<()>)>,
    audio: Option<gunshots::GunshotThread>,
    host_status: Arc<ro_core::hosting::AgentStatus>,
    bus: ro_core::hosting::GameEventBus,
}

impl Services {
    pub fn start(host: &Host, runtime: &Handle, feed: Arc<Mutex<EventFeed>>, notify_feed: Notify) -> Self {
        let notify_feed_for_audio = notify_feed.clone();
        let subscription = event_pump::subscribe(&host.bus);

        let uplink = RelayUplink::new(
            host.active_team.clone(),
            host.tokens.clone(),
            host.loadout.clone(),
            host.room.clone(),
            host.bus.clone(),
            host.options.relay.clone(),
        );

        let tasks = vec![
            runtime.spawn(log_tail::run(
                host.locator.clone(),
                host.bus.clone(),
                host.status.clone(),
                host.options.agent.clone(),
            )),
            runtime.spawn(async move { uplink.run().await }),
            runtime.spawn(event_pump::run(subscription, feed, notify_feed)),
            runtime.spawn(updates::run()),
        ];

        let vision = VisionService::new(host.locator.clone(), host.frames.clone(), host.loadout.clone(), host.diagnostics.clone())
            .with_skin_icons(skin_icons::SkinIcons::start(host, runtime));
        let stop = StopToken::new();
        let stopping = stop.clone();
        let (finished, done) = mpsc::channel();
        std::thread::Builder::new()
            .name("vision".into())
            .spawn(move || {
                crate::interop::thread_priority::lower_current_thread();
                vision.run(&stopping);
                let _ = finished.send(());
            })
            .expect("spawning the capture thread");

        let audio = gunshots::start(host, notify_feed_for_audio);

        Self { tasks, vision: Some((stop, done)), audio: Some(audio), host_status: host.status.clone(), bus: host.bus.clone() }
    }

    /// Stops everything. Dropping the relay task marks the room disconnected on its way out.
    pub fn shutdown(&mut self) {
        for task in self.tasks.drain(..) {
            task.abort();
        }

        if let Some((stop, done)) = self.vision.take() {
            stop.stop();
            if let Err(RecvTimeoutError::Timeout) = done.recv_timeout(STOP_TIMEOUT) {
                tracing::warn!("The capture thread did not stop in time; leaving it to process exit");
            }
        }
        if let Some(audio) = self.audio.take() {
            audio.stop(STOP_TIMEOUT);
        }

        self.host_status.set(TailState::Idle, "");
        self.bus.complete();
    }
}
