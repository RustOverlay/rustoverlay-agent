use super::{FrameBuffer, HudGeometry, Rgb, VitalsRow};

/// One vitals row's measurement, before it is turned into a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarMeasurement {
    pub left_x: i32,
    pub width: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VitalsReading {
    pub health: Option<f64>,
    pub hydration: Option<f64>,
    pub calories: Option<f64>,
    pub scale: f64,
    pub confidence: f64,
}

/// Track width at the reference frame, 1920 wide at uiscale 1.0.
const REFERENCE_TRACK: f64 = 247.0;

const REFERENCE_BAR_LEFT_FROM_RIGHT: f64 = 274.0;

// Row maxima. Health is the only one that is also the displayed maximum.
const HEALTH_MAX: i32 = 100;
const HYDRATION_MAX: i32 = 250;
const CALORIES_MAX: i32 = 500;

/// Reads the vitals from the width of their bars rather than from the digits printed on them.
/// See docs/hud-layout.md.
///
/// Deliberately not OCR: at low values the digits straddle the bar's right edge, half on the bar
/// and half on whatever the game draws behind it — the worst input for recognition and the case
/// that matters most. The bar edge has no such problem.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VitalsReader {
    ui_scale: f64,
}

impl Default for VitalsReader {
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl VitalsReader {
    pub fn new(ui_scale: f64) -> Self {
        Self { ui_scale }
    }

    /// None when the health bar cannot be found, which means the HUD is not on screen.
    pub fn read<F: FrameBuffer + ?Sized>(&self, frame: &F) -> Option<VitalsReading> {
        let hud = HudGeometry::new(frame.width(), frame.height(), self.ui_scale);

        let health = measure(frame, hud.health, is_health)?;

        // The left edge does not move with the value; anchoring on it absorbs uiScale/model error.
        let scale = f64::from(frame.width() - 1 - health.left_x) / REFERENCE_BAR_LEFT_FROM_RIGHT;
        let track = REFERENCE_TRACK * scale;

        let hydration = measure(frame, hud.hydration, is_hydration);
        let calories = measure(frame, hud.calories, is_calories);

        let drift = (health.left_x - hud.health.left_x).abs();

        Some(VitalsReading {
            health: value(Some(health), track, HEALTH_MAX),
            hydration: value(hydration, track, HYDRATION_MAX),
            calories: value(calories, track, CALORIES_MAX),
            scale,
            // Geometry agreement only; a digit cross-check is what would make this a confidence.
            confidence: if f64::from(drift) <= 3.0 * scale { 1.0 } else { 0.5 },
        })
    }
}

fn value(bar: Option<BarMeasurement>, track: f64, max: i32) -> Option<f64> {
    bar.map(|b| (f64::from(b.width) / track * f64::from(max)).clamp(0.0, f64::from(max)))
}

/// Finds the horizontal extent of the row's colour by voting each column over the row's full
/// height. Constraining the scan to the row band is what makes hue matching safe — grass is green,
/// and terrain is full of oranges.
///
/// Deliberately not a contiguous run: the value's digits are painted on the bar, so the longest
/// unbroken piece is the tail after them, which reads the left edge ~12% too far right. Column
/// voting tolerates the digits because they leave colour above and below.
fn measure<F: FrameBuffer + ?Sized>(frame: &F, row: VitalsRow, is_bar: fn(Rgb) -> bool) -> Option<BarMeasurement> {
    let top = 0.max(row.top);
    let bottom = (frame.height() - 1).min(row.bottom());
    if bottom - top < 2 {
        return None;
    }

    let height = bottom - top + 1;
    let threshold = 2.max(height / 4);

    let margin = 8.max(height * 2);
    let from = 0.max(row.left_x - margin);
    let to = (frame.width() - 1).min(row.left_x + (REFERENCE_TRACK * 1.5) as i32 + margin);

    let (mut first, mut last) = (-1, -1);

    for x in from..=to {
        let mut hits = 0;
        let mut y = top;
        while y <= bottom && hits < threshold {
            if is_bar(frame.pixel(x, y)) {
                hits += 1;
            }
            y += 1;
        }

        if hits < threshold {
            continue;
        }

        if first < 0 {
            first = x;
        }
        last = x;
    }

    // A few stray columns of matching terrain is not a bar.
    if first < 0 || last - first + 1 < 4 { None } else { Some(BarMeasurement { left_x: first, width: last - first + 1 }) }
}

fn is_health(c: Rgb) -> bool {
    let (r, g, b) = (i32::from(c.r), i32::from(c.g), i32::from(c.b));
    g > 120 && g - r > 25 && g - b > 60
}

fn is_hydration(c: Rgb) -> bool {
    let (r, g, b) = (i32::from(c.r), i32::from(c.g), i32::from(c.b));
    b > 130 && b - r > 50 && b - g > 20
}

fn is_calories(c: Rgb) -> bool {
    let (r, g, b) = (i32::from(c.r), i32::from(c.g), i32::from(c.b));
    r > 150 && r - g > 45 && g - b > 25
}
