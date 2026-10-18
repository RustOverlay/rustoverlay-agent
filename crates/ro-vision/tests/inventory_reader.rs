//! Synthetic icons painted into a synthetic frame, covering what the reader adds over the matcher:
//! which containers it visits, and that a cached slot is dropped the moment its pixels move.

mod common;

use std::sync::Arc;

use common::{item, string_hash, N};
use ro_vision::hud::{
    inventory_is_open, CountReader, FrameBuffer, GlyphAtlas, GlyphSet, InventoryGeometry, InventoryReader, Rgb, SlotRect,
};
use ro_vision::items::{Catalogue, IconMatcher, IconSignature};

struct Canvas {
    width: i32,
    height: i32,
    rgb: Vec<u8>,
}

impl FrameBuffer for Canvas {
    fn width(&self) -> i32 {
        self.width
    }

    fn height(&self) -> i32 {
        self.height
    }

    fn pixel(&self, x: i32, y: i32) -> Rgb {
        let i = ((y * self.width + x) * 3) as usize;
        Rgb::new(self.rgb[i], self.rgb[i + 1], self.rgb[i + 2])
    }
}

impl Canvas {
    fn new(width: i32, height: i32) -> Self {
        Self { width, height, rgb: vec![0; (width * height * 3) as usize] }
    }

    fn fill_slot(&mut self, area: SlotRect, paint: impl Fn(i32, i32) -> Rgb) {
        self.fill(area.x, area.y, area.size, area.size, paint);
    }

    fn fill(&mut self, left: i32, top: i32, w: i32, h: i32, paint: impl Fn(i32, i32) -> Rgb) {
        for y in 0.max(top)..self.height.min(top + h) {
            for x in 0.max(left)..self.width.min(left + w) {
                let c = paint(x - left, y - top);
                let i = ((y * self.width + x) * 3) as usize;
                (self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]) = (c.r, c.g, c.b);
            }
        }
    }
}

/// A glyph atlas with no glyphs, so counts never read and the icon side is isolated.
struct NoGlyphs;

impl GlyphAtlas for NoGlyphs {
    fn at(&self, _digit_height: i32) -> Arc<GlyphSet> {
        Arc::new(GlyphSet::new())
    }
}

/// Uncorrelated per-cell noise, so each item matches itself and nothing else.
fn cells(short_name: &str) -> Vec<f32> {
    let mut seed = string_hash(short_name) | 1;

    (0..N * N * 3)
        .map(|_| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            if (seed >> 24).is_multiple_of(2) { 0.0 } else { 1.0 }
        })
        .collect()
}

/// Paints an item into a slot by blowing its cells back up to the icon area.
fn paint(canvas: &mut Canvas, slot: SlotRect, short_name: &str) {
    canvas.fill_slot(slot, |_, _| Rgb::new(60, 60, 60));

    let icon = slot.icon_area();
    let cells = cells(short_name);
    let n = N as i32;

    canvas.fill_slot(icon, |x, y| {
        let cell = (((n - 1).min(y * n / icon.size) * n + (n - 1).min(x * n / icon.size)) * 3) as usize;
        let byte = |v: f32| (v * 255.0) as u8;
        Rgb::new(byte(cells[cell]), byte(cells[cell + 1]), byte(cells[cell + 2]))
    });
}

fn fixture(belt: &[&str]) -> (InventoryReader, Canvas, InventoryGeometry) {
    let catalogue: Catalogue = ["alpha", "beta", "gamma"]
        .into_iter()
        .map(|name| (item(name).into(), IconSignature::new(cells(name), vec![1.0; N * N])))
        .collect();

    let geometry = InventoryGeometry::new(1920, 1080, 1.0);
    let mut canvas = Canvas::new(1920, 1080);

    for (i, name) in belt.iter().enumerate() {
        paint(&mut canvas, geometry.belt()[i], name);
    }

    let reader = InventoryReader::new(Arc::new(IconMatcher::new(catalogue)), Arc::new(CountReader::new(Arc::new(NoGlyphs))));

    (reader, canvas, geometry)
}

/// Paints the slot lattice: slots one shade, the gaps between them darker.
fn paint_panel(canvas: &mut Canvas, geometry: &InventoryGeometry) {
    let grid = geometry.main();
    let (first, last) = (grid[0], grid[grid.len() - 1]);
    canvas.fill(first.x, first.y, last.right() - first.x + 1, last.bottom() - first.y + 1, |_, _| Rgb::new(20, 20, 20));

    for &slot in grid {
        canvas.fill_slot(slot, |_, _| Rgb::new(90, 90, 90));
    }
}

fn belt_names(reading: &ro_vision::hud::InventoryReading) -> Vec<Option<&str>> {
    reading.belt.iter().map(|s| s.item.as_ref().map(|m| m.item.short_name.as_str())).collect()
}

#[test]
fn the_panel_is_detected_from_the_gaps_between_slots() {
    let geometry = InventoryGeometry::new(1920, 1080, 1.0);
    let mut canvas = Canvas::new(1920, 1080);
    paint_panel(&mut canvas, &geometry);

    assert!(inventory_is_open(&canvas, &geometry));
}

#[test]
fn a_uniform_frame_is_not_mistaken_for_the_panel() {
    // Snow and night sky are as flat as any UI panel, which is why every measure of how uniform the
    // region looks was abandoned. Only an edge at the predicted coordinate counts.
    let geometry = InventoryGeometry::new(1920, 1080, 1.0);
    let mut canvas = Canvas::new(1920, 1080);
    canvas.fill(0, 0, 1920, 1080, |_, _| Rgb::new(240, 240, 245));

    assert!(!inventory_is_open(&canvas, &geometry));
}

#[test]
fn a_shut_inventory_reads_no_main_grid_slots_at_all() {
    // Pointed at world content the matcher invents items, so the containers must not be read at
    // all rather than read and filtered.
    let (mut reader, canvas, geometry) = fixture(&["alpha"]);

    let reading = reader.read(&canvas, &geometry);

    assert!(reading.main.is_empty());
    assert!(reading.backpack.is_none());
}

#[test]
fn each_belt_slot_reads_the_item_painted_into_it() {
    let (mut reader, canvas, geometry) = fixture(&["alpha", "beta", "gamma"]);

    let reading = reader.read_with_panel(&canvas, &geometry, false);

    assert_eq!(belt_names(&reading)[..3], [Some("alpha"), Some("beta"), Some("gamma")]);
}

#[test]
fn a_slot_whose_pixels_changed_is_read_again_rather_than_served_stale() {
    let (mut reader, mut canvas, geometry) = fixture(&["alpha"]);

    assert_eq!(belt_names(&reader.read_with_panel(&canvas, &geometry, false))[0], Some("alpha"));

    paint(&mut canvas, geometry.belt()[0], "beta");

    assert_eq!(belt_names(&reader.read_with_panel(&canvas, &geometry, false))[0], Some("beta"));
}

#[test]
fn an_unchanged_frame_reads_the_same_twice() {
    let (mut reader, canvas, geometry) = fixture(&["alpha", "beta"]);

    let first = reader.read_with_panel(&canvas, &geometry, false);
    let second = reader.read_with_panel(&canvas, &geometry, false);

    assert_eq!(first.belt, second.belt);
}

#[test]
fn a_closed_inventory_reads_the_belt_only() {
    let (mut reader, canvas, geometry) = fixture(&["alpha"]);

    let reading = reader.read_with_panel(&canvas, &geometry, false);

    assert_eq!(reading.belt.len(), InventoryGeometry::BELT_SLOTS);
    assert!(reading.main.is_empty());
    assert!(reading.wear.is_empty());
    assert!(reading.backpack.is_none());
}

#[test]
fn a_zero_size_frame_reads_nothing() {
    let (mut reader, _, geometry) = fixture(&[]);

    assert_eq!(reader.read(&Canvas::new(0, 0), &geometry), Default::default());
}

#[test]
fn a_slot_hanging_off_the_frame_abstains_rather_than_reading_past_it() {
    // Geometry for 1080 rows against a 1000-row capture, as mid-resize: the belt sits below row 1000.
    let (mut reader, _, geometry) = fixture(&[]);

    let reading = reader.read_with_panel(&Canvas::new(1920, 1000), &geometry, true);

    assert_eq!(reading.belt.len(), InventoryGeometry::BELT_SLOTS);
    assert!(reading.belt.iter().all(|s| s.item.is_none()));
}

#[test]
fn an_open_inventory_reads_every_container() {
    let (mut reader, canvas, geometry) = fixture(&["alpha"]);

    let reading = reader.read_with_panel(&canvas, &geometry, true);

    assert_eq!(reading.main.len(), InventoryGeometry::MAIN_COLUMNS * InventoryGeometry::MAIN_ROWS);
    assert_eq!(reading.wear.len(), InventoryGeometry::WEAR_SLOTS);
    assert!(reading.backpack.is_some());
}
