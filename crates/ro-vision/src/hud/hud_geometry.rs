/// One vitals row: where its bar starts and the horizontal band it occupies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VitalsRow {
    /// X of the bar's left edge, which is fixed. The bar grows rightward from here.
    pub left_x: i32,
    pub top: i32,
    pub height: i32,
}

impl VitalsRow {
    pub const fn bottom(&self) -> i32 {
        self.top + self.height - 1
    }
}

// Reference frame: 1920x1080 at graphics.uiscale 1.0.
const REFERENCE_WIDTH: f64 = 1920.0;
const BAR_LEFT_FROM_RIGHT: f64 = 274.0;
const HEALTH_BOTTOM_FROM_BOTTOM: f64 = 113.0;
const BAR_HEIGHT: f64 = 30.0;
const ROW_PITCH: f64 = 42.0;

/// Where the vitals cluster sits for a given frame size and UI scale. See docs/hud-layout.md.
///
/// A search hint, not gospel: the scaling model is derived from two aspect ratios sampled at the
/// same height, so a locator must confirm by finding the bar edge. Offsets are anchored to the
/// bottom-right corner because the cluster is, and status banners stack upward without moving it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudGeometry {
    pub scale: f64,
    pub health: VitalsRow,
    pub hydration: VitalsRow,
    pub calories: VitalsRow,
}

impl HudGeometry {
    pub fn new(frame_width: i32, frame_height: i32, ui_scale: f64) -> Self {
        // Width, not height: a 1440x1080 frame scales by 1440/1920, not by 1080/1080.
        let scale = f64::from(frame_width) / REFERENCE_WIDTH * ui_scale;

        let left = frame_width - 1 - round(BAR_LEFT_FROM_RIGHT * scale);
        let height = 1.max(round(BAR_HEIGHT * scale));
        let pitch = round(ROW_PITCH * scale);
        let health_bottom = frame_height - 1 - round(HEALTH_BOTTOM_FROM_BOTTOM * scale);

        let row = |bottom: i32| VitalsRow { left_x: left, top: bottom - height + 1, height };

        Self {
            scale,
            health: row(health_bottom),
            hydration: row(health_bottom + pitch),
            calories: row(health_bottom + pitch * 2),
        }
    }
}

/// `MidpointRounding.AwayFromZero`, which is what `f64::round` does.
fn round(value: f64) -> i32 {
    value.round() as i32
}
