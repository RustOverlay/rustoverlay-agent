//! Synthetic frames, so the arithmetic and the row scanning are covered without needing an image
//! decoder. Real captures are exercised by the capture smoke test.

use ro_vision::hud::{FrameBuffer, HudGeometry, Rgb, VitalsReader, VitalsRow};

const HEALTH: Rgb = Rgb::new(140, 200, 60);
const HYDRATION: Rgb = Rgb::new(60, 150, 230);
const CALORIES: Rgb = Rgb::new(210, 120, 55);
const BACKGROUND: Rgb = Rgb::new(30, 32, 38);

struct FakeFrame {
    width: i32,
    height: i32,
    pixels: Vec<Rgb>,
}

impl FakeFrame {
    fn new(width: i32, height: i32, fill: Rgb) -> Self {
        Self { width, height, pixels: vec![fill; (width * height) as usize] }
    }

    fn set(&mut self, x: i32, y: i32, colour: Rgb) {
        self.pixels[(y * self.width + x) as usize] = colour;
    }
}

impl FrameBuffer for FakeFrame {
    fn width(&self) -> i32 {
        self.width
    }

    fn height(&self) -> i32 {
        self.height
    }

    fn pixel(&self, x: i32, y: i32) -> Rgb {
        self.pixels[(y * self.width + x) as usize]
    }
}

/// Paints bars of the given values at the geometry the reader expects to find them.
fn frame(width: i32, height: i32, ui_scale: f64, hp: Option<i32>, water: Option<i32>, food: Option<i32>) -> FakeFrame {
    let mut frame = FakeFrame::new(width, height, BACKGROUND);
    let hud = HudGeometry::new(width, height, ui_scale);
    let track = 247.0 * hud.scale;

    let mut paint = |row: VitalsRow, value: Option<i32>, max: i32, colour: Rgb| {
        let Some(value) = value else { return };

        // C#'s Math.Round, which rounds half to even.
        let bar_width = (f64::from(value) / f64::from(max) * track).round_ties_even() as i32;
        for y in row.top..=row.bottom() {
            for x in row.left_x..row.left_x + bar_width {
                frame.set(x, y, colour);
            }
        }
    };

    paint(hud.health, hp, 100, HEALTH);
    paint(hud.hydration, water, 250, HYDRATION);
    paint(hud.calories, food, 500, CALORIES);

    frame
}

fn near(expected: f64, actual: f64, tolerance: f64) {
    assert!((expected - actual).abs() <= tolerance, "expected {expected} +/- {tolerance}, got {actual}");
}

#[test]
fn recovers_values_at_any_resolution_and_ui_scale() {
    for (width, height, ui_scale) in [(1920, 1080, 1.0), (1440, 1080, 1.0), (1600, 900, 1.0), (1920, 1080, 0.5)] {
        let reading = VitalsReader::new(ui_scale)
            .read(&frame(width, height, ui_scale, Some(39), Some(62), Some(51)))
            .expect("the HUD is drawn");

        near(39.0, reading.health.unwrap(), 1.0);
        near(62.0, reading.hydration.unwrap(), 1.5);
        near(51.0, reading.calories.unwrap(), 3.0);
    }
}

#[test]
fn returns_none_when_the_hud_is_absent() {
    assert_eq!(VitalsReader::default().read(&FakeFrame::new(1920, 1080, BACKGROUND)), None);
}

#[test]
fn reads_the_other_rows_even_when_one_is_empty() {
    let reading = VitalsReader::default().read(&frame(1920, 1080, 1.0, Some(80), Some(40), None)).expect("the HUD is drawn");

    near(80.0, reading.health.unwrap(), 1.0);
    assert_eq!(reading.calories, None);
}

#[test]
fn reads_past_the_digits_painted_on_the_bar() {
    // The value is drawn on top of its own bar, breaking the colour into pieces. Measuring the
    // longest unbroken piece finds the tail after the digits, ~12% too far right.
    let mut frame = frame(1920, 1080, 1.0, Some(39), Some(62), Some(51));
    let hud = HudGeometry::new(1920, 1080, 1.0);

    for x in hud.health.left_x + 6..hud.health.left_x + 30 {
        for y in hud.health.top + 5..=hud.health.bottom() - 5 {
            frame.set(x, y, Rgb::new(255, 255, 255));
        }
    }

    let reading = VitalsReader::default().read(&frame).expect("the HUD is drawn");

    near(39.0, reading.health.unwrap(), 1.0);
    near(1.0, reading.scale, 0.02);
}

#[test]
fn ignores_matching_colour_outside_the_row_bands() {
    // Grass is green. A bar-shaped run of it well above the HUD must not be mistaken for health.
    let mut frame = frame(1920, 1080, 1.0, Some(39), Some(62), Some(51));
    for x in 1600..1800 {
        frame.set(x, 500, HEALTH);
    }

    let reading = VitalsReader::default().read(&frame).expect("the HUD is drawn");

    near(39.0, reading.health.unwrap(), 1.0);
}
