use super::{Packet, FLAG_QPC_ESTIMATED, FLAG_TIMESTAMP_ERROR};

/// Gaps shorter than this are packet-timestamp jitter, not missing audio.
const GAP_TOLERANCE_100NS: f64 = 20_000.0;

/// Retrieval-time estimates jitter by wake-up latency, so only a jump past ~1.5 engine periods
/// counts as a gap; smaller deviations nudge the running timeline by `ESTIMATE_GAIN` instead.
const ESTIMATE_GAP_TOLERANCE_100NS: f64 = 150_000.0;
const ESTIMATE_GAIN: f64 = 0.05;

/// Silence the stream skipped: loopback delivers nothing while nothing renders.
#[derive(Clone, Copy, Debug)]
pub struct Gap {
    /// Where the timeline expected the next packet.
    pub qpc_100ns: i64,
    pub frames: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct Placement {
    /// The packet's QPC (smoothed when it was a retrieval-time estimate).
    pub qpc_100ns: i64,
    /// Where its first frame sits on the timeline: `qpc_100ns`, or the extrapolated position when
    /// the engine flagged the timestamp as bad.
    pub start_qpc_100ns: i64,
    /// Padding owed before this packet to keep the frame count continuous with QPC.
    pub gap: Option<Gap>,
}

/// Places packets on a continuous QPC timeline, re-anchored on every timestamped packet so
/// sample-clock drift never accumulates.
pub struct Timeline {
    rate: f64,
    expected_qpc: Option<f64>,
}

impl Timeline {
    pub fn new(rate: u32) -> Self {
        Self { rate: f64::from(rate), expected_qpc: None }
    }

    pub fn place(&mut self, packet: &Packet<'_>) -> Placement {
        let estimated = packet.flags & FLAG_QPC_ESTIMATED != 0;
        let qpc = if estimated { self.smooth_estimate(packet.qpc_100ns as f64) } else { packet.qpc_100ns as i64 };
        let tolerance = if estimated { ESTIMATE_GAP_TOLERANCE_100NS } else { GAP_TOLERANCE_100NS };
        let timestamp_ok = packet.flags & FLAG_TIMESTAMP_ERROR == 0;

        let gap = match (self.expected_qpc, timestamp_ok) {
            (Some(expected), true) if qpc as f64 - expected > tolerance => Some(Gap {
                qpc_100ns: expected as i64,
                frames: ((qpc as f64 - expected) * self.rate / 1e7).round() as u64,
            }),
            _ => None,
        };

        let start = if timestamp_ok { qpc as f64 } else { self.expected_qpc.unwrap_or(qpc as f64) };
        self.expected_qpc = Some(start + f64::from(packet.frames) * 1e7 / self.rate);
        Placement { qpc_100ns: qpc, start_qpc_100ns: start as i64, gap }
    }

    /// Within tolerance of the running timeline, an estimate only nudges it, which averages out
    /// wake-up jitter while still tracking sample-clock drift. Past it, the raw estimate stands and
    /// the caller pads up to it.
    fn smooth_estimate(&self, estimate: f64) -> i64 {
        match self.expected_qpc {
            Some(expected) if estimate - expected <= ESTIMATE_GAP_TOLERANCE_100NS => {
                (expected + (estimate - expected) * ESTIMATE_GAIN) as i64
            }
            _ => estimate as i64,
        }
    }
}
