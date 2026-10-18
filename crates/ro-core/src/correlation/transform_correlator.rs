use chrono::TimeDelta;

use crate::events::{CameraTransform, TransformTriple};

/// Pairs the two triples produced by a `printpos;printeyes` keybind into one [`CameraTransform`].
///
/// Neither command labels its output — the console echoes only a bare `(a, b, c)` — so meaning
/// comes from emission order, which the agent controls by owning the keybind it tells the user to
/// set. Order alone is not enough when the user runs a command by hand, so a candidate that cannot
/// be an orientation is rejected and treated as a fresh position. See docs/log-format.md.
#[derive(Debug)]
pub struct TransformCorrelator {
    window: TimeDelta,
    pending: Option<TransformTriple>,
}

impl Default for TransformCorrelator {
    fn default() -> Self {
        Self::new(None)
    }
}

impl TransformCorrelator {
    pub fn new(window: Option<TimeDelta>) -> Self {
        Self { window: window.unwrap_or(TimeDelta::milliseconds(500)), pending: None }
    }

    pub fn accept(&mut self, triple: TransformTriple) -> Option<CameraTransform> {
        let pairs = self
            .pending
            .as_ref()
            .is_some_and(|p| triple.timestamp - p.timestamp <= self.window && is_orientation(&triple));

        if !pairs {
            self.pending = Some(triple);
            return None;
        }

        let position = self.pending.take()?;

        Some(CameraTransform {
            timestamp: position.timestamp,
            x: position.a,
            y: position.b,
            z: position.c,
            raw_pitch: triple.a,
            yaw: triple.b,
            roll: triple.c,
        })
    }

    /// Drops a triple left waiting when the stream goes quiet.
    pub fn reset(&mut self) {
        self.pending = None;
    }
}

/// Euler angles are logged in [0, 360) and roll is 0 outside vehicles. World coordinates go
/// negative and exceed 360 on any map above 720m, so a triple that breaks the angle range is
/// certainly a position.
fn is_orientation(t: &TransformTriple) -> bool {
    in_angle_range(t.a) && in_angle_range(t.b) && in_angle_range(t.c)
}

fn in_angle_range(value: f64) -> bool {
    (0.0..360.0).contains(&value)
}
