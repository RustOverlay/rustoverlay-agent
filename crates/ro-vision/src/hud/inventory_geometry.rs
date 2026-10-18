#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SlotRect {
    pub x: i32,
    pub y: i32,
    pub size: i32,
}

/// Drawn icon size, as a fraction of slot size.
const ICON_SCALE: f64 = 0.806;

/// Top of the icon below the top of the slot, as a fraction of slot size.
const ICON_TOP: f64 = 0.044;

impl SlotRect {
    pub const fn new(x: i32, y: i32, size: i32) -> Self {
        Self { x, y, size }
    }

    pub const fn right(&self) -> i32 {
        self.x + self.size - 1
    }

    pub const fn bottom(&self) -> i32 {
        self.y + self.size - 1
    }

    /// Whether every pixel of the rect is inside a `width` x `height` frame.
    pub const fn lies_within(&self, width: i32, height: i32) -> bool {
        self.x >= 0 && self.y >= 0 && self.size > 0 && self.right() < width && self.bottom() < height
    }

    /// Where the item icon is actually drawn. Matching against the slot rect itself scores far
    /// worse — the icon neither fills the slot nor is centred in it, and the slot's edges carry the
    /// condition bar and the stack or ammo count instead.
    ///
    /// Swept exhaustively rather than fitted by hand: 16,170 candidate rects scored against
    /// belt-corpus, every one of the top 25 agreeing on the same drawn rect. Size is taken directly
    /// rather than as `size - 2 * inset`, which can only produce an even change. See
    /// docs/hud-layout.md.
    pub fn icon_area(&self) -> SlotRect {
        // Gotcha: C#'s default Math.Round is banker's rounding, not f64::round.
        let size = (f64::from(self.size) * ICON_SCALE).round_ties_even() as i32;
        let top = (f64::from(self.size) * ICON_TOP).round_ties_even() as i32;

        SlotRect::new(self.x + (self.size - size) / 2, self.y + top, size)
    }
}

const REFERENCE_WIDTH: f64 = 1920.0;
const REFERENCE_HEIGHT: f64 = 1080.0;

// Pitch matches gameui.prefab exactly; size stays the fitted 91, not the prefab's 90 — see icon_area.
const SLOT_PITCH: f64 = 96.0; // 64 * 1.5
const SLOT_SIZE: f64 = 91.0; // 68 / 0.75
const WEAR_PITCH: f64 = 81.0; // 61 / 0.75
const BELT_BOTTOM_FROM_BOTTOM: f64 = 27.0; // 20 / 0.75 and 22 / 0.833 agree on ~26.5
const BELT_LEFT_AT_1920: f64 = 660.0; // 495 / 0.75
const MAIN_LEFT_AT_1920: f64 = 659.0; // 494 / 0.75
// Gotcha: the comb fit locks onto the separator above the row, ~4px above the slot interior.
const MAIN_TOP_ROW_FROM_BOTTOM: f64 = 509.0; // 1079 - 929*0.75, converted to the reference
const WEAR_LEFT_AT_1920: f64 = 79.0; // 59 / 0.75
const WEAR_TOP_FROM_BOTTOM: f64 = 249.0; // 1079 - 892*0.75; aligned against known armour

/// Where the belt, main inventory and wear slots sit. See docs/hud-layout.md.
///
/// Everything is anchored to the frame's bottom-left and scaled by `scale`, which is Unity's
/// `CanvasScaler` in `Expand` mode: the smaller of the two axis ratios against the 1920x1080
/// reference. Width alone is correct at 16:9 and narrower, but at 2560x1080 it gives a 128px pitch
/// where the game draws 96.
///
/// Gotcha: **every** container is measured from the bottom. Offsets from the top happen to agree
/// at 1440x1080 and disagree everywhere else — symptom: the belt reads perfectly while another
/// container abstains on every slot.
#[derive(Clone, Debug, PartialEq)]
pub struct InventoryGeometry {
    scale: f64,
    frame_width: i32,
    frame_height: i32,
    belt: Vec<SlotRect>,
    main: Vec<SlotRect>,
    wear: Vec<SlotRect>,
    backpack: SlotRect,
}

impl InventoryGeometry {
    pub const BELT_SLOTS: usize = 6;
    pub const MAIN_COLUMNS: usize = 6;
    pub const MAIN_ROWS: usize = 4;
    pub const WEAR_SLOTS: usize = 7;

    pub fn new(frame_width: i32, frame_height: i32, ui_scale: f64) -> Self {
        let scale = (f64::from(frame_width) / REFERENCE_WIDTH).min(f64::from(frame_height) / REFERENCE_HEIGHT) * ui_scale;
        let r = |v: f64| round(v * scale);

        let size = r(SLOT_SIZE);
        let pitch = r(SLOT_PITCH);

        let belt_bottom = frame_height - 1 - r(BELT_BOTTOM_FROM_BOTTOM);
        let belt = row(r(BELT_LEFT_AT_1920), belt_bottom - size + 1, pitch, size, Self::BELT_SLOTS);

        let main_left = r(MAIN_LEFT_AT_1920);
        let main_top = frame_height - 1 - r(MAIN_TOP_ROW_FROM_BOTTOM);
        let main = (0..Self::MAIN_ROWS as i32)
            .flat_map(|line| row(main_left, main_top + line * pitch, pitch, size, Self::MAIN_COLUMNS))
            .collect();

        let wear_size = r(WEAR_PITCH) - r(5.0);
        let wear_top = frame_height - 1 - r(WEAR_TOP_FROM_BOTTOM);
        let wear_pitch = r(WEAR_PITCH);
        let wear_left = r(WEAR_LEFT_AT_1920);

        Self {
            scale,
            frame_width,
            frame_height,
            belt,
            main,
            wear: row(wear_left, wear_top, wear_pitch, wear_size, Self::WEAR_SLOTS),
            backpack: SlotRect::new(wear_left, wear_top - wear_pitch, wear_size),
        }
    }

    pub fn scale(&self) -> f64 {
        self.scale
    }

    pub fn frame_width(&self) -> i32 {
        self.frame_width
    }

    pub fn frame_height(&self) -> i32 {
        self.frame_height
    }

    /// Six slots, bottom centre. Visible in world view as well as in the inventory — the only
    /// container readable without the inventory open, and the one carrying the held item.
    pub fn belt(&self) -> &[SlotRect] {
        &self.belt
    }

    /// 24 slots, row-major from the top left.
    pub fn main(&self) -> &[SlotRect] {
        &self.main
    }

    /// Seven armour slots. A smaller pitch than the other containers — 81 against 96.
    pub fn wear(&self) -> &[SlotRect] {
        &self.wear
    }

    /// The backpack, one wear pitch directly above the first armour slot. It shows a fill count,
    /// `0/28`, not a stack count, so `CountReader` does not apply to it.
    pub fn backpack(&self) -> SlotRect {
        self.backpack
    }
}

fn row(left: i32, top: i32, pitch: i32, size: i32, count: usize) -> Vec<SlotRect> {
    (0..count as i32).map(|i| SlotRect::new(left + i * pitch, top, size)).collect()
}

/// `MidpointRounding.AwayFromZero`, which is what `f64::round` does.
fn round(v: f64) -> i32 {
    v.round() as i32
}
