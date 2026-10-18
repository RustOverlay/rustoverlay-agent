//! One frame's recognition pass, and the diagnostics hung off it.

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use ro_vision::hosting::LoadoutState;
use ro_vision::hud::{
    inventory_is_open, selected_slot, slot_hash, CountRejection, FrameBuffer, InventoryGeometry, InventoryReader,
    InventoryReading, SlotCount, SlotHistory, SlotRect, VitalsReader,
};
use ro_vision::items::{is_default_skin_category, FrameSlotSampler, IconMatcher, SlotSampler};

use crate::fetched_skins::SkinIconSource;
use crate::frame_source::{CaptureError, FrameSource};
use crate::skin_hunt::{self, SkinHunt};
use crate::{display_scaling, frame_image, rust_window};

/// Gap between saved count-rejection frames. Forensics, not a monitor.
const REJECTION_CAPTURE: Duration = Duration::from_secs(60);

pub(crate) struct VisionPass {
    pub reader: InventoryReader,
    history: SlotHistory,
    hunt: SkinHunt,
    vitals: VitalsReader,
    ui_scale: f64,
    /// Logged once, not once a second, since the condition persists until the user acts.
    warned_about_scaling: bool,
    last_rejection_capture: Option<SystemTime>,
}

impl VisionPass {
    pub fn new(reader: InventoryReader, ui_scale: f64) -> Self {
        Self {
            reader,
            history: SlotHistory::new(),
            hunt: SkinHunt::default(),
            vitals: VitalsReader::new(ui_scale),
            ui_scale,
            warned_about_scaling: false,
            last_rejection_capture: None,
        }
    }

    /// Whether the screen can be read at all, or whether DPI virtualisation would put the crop in
    /// the wrong place. See [`display_scaling`].
    ///
    /// Rechecked each read rather than once, because moving the window to another display changes
    /// the answer — including back to a working one, which is why nothing latches.
    pub fn coordinates_are_real(&mut self) -> bool {
        let Some(dpi) = display_scaling::virtualises_coordinates(rust_window::find_handle()) else { return true };

        if !self.warned_about_scaling {
            tracing::error!(
                "Display is scaled to {} but this process is not per-monitor DPI aware, so screen coordinates are \
                 virtualised; pausing rather than reading the wrong pixels",
                percent(dpi)
            );
            self.warned_about_scaling = true;
        }

        false
    }

    /// The whole recognition pass runs inside the callback because the frame is only valid for
    /// its duration — see [`FrameSource::read`].
    pub fn read(
        &mut self,
        frames: &dyn FrameSource,
        loadout: &Mutex<LoadoutState>,
        data_directory: &Path,
        now: SystemTime,
        skins: Option<&dyn SkinIconSource>,
    ) -> Result<bool, CaptureError> {
        frames.read(&mut |frame| {
            let geometry = InventoryGeometry::new(frame.width(), frame.height(), self.ui_scale);

            // Detected once and passed down, so the reading and the flag published beside it cannot
            // disagree about which frame they describe.
            let open = inventory_is_open(frame, &geometry);
            let reading = self.reader.read_with_panel(frame, &geometry, open);
            if let Some(source) = skins
                && !open
            {
                self.hunt_skins(frame, &reading, source);
            }
            let confirmed = self.history.confirm(frame, &reading, open, now);

            let rejections = self.history.last_count_rejections();
            if !rejections.is_empty() {
                record_rejections(frame, rejections, now, &mut self.last_rejection_capture, data_directory);
            }

            let held = selected_slot(frame, &geometry);
            let vitals = self.vitals.read(frame);
            loadout.lock().unwrap_or_else(PoisonError::into_inner).set(confirmed, held, vitals, open, now);
        })
    }

    /// Asks for the skins of whatever item an unread, non-empty belt slot keeps resembling. Only the
    /// belt over the world: the panel's grey is read reliably, and it is the belt the team sees.
    fn hunt_skins<F: FrameBuffer + ?Sized>(&mut self, frame: &F, reading: &InventoryReading, source: &dyn SkinIconSource) {
        let matcher = Arc::clone(self.reader.matcher());
        let now = Instant::now();

        for (index, slot) in reading.belt.iter().enumerate() {
            let leaders = if slot.item.is_none() { skinnable_leaders(frame, slot.slot, &matcher, source) } else { None };
            if let Some(item) = self.hunt.observe(index, slot_hash(frame, slot.slot), leaders, now) {
                tracing::info!(slot = index, item, "An unread belt slot keeps resembling a skinnable item; asking for its skins");
                source.request(&item);
            }
        }
    }
}

/// The skinnable items among the slot's top [`skin_hunt::TOP`], best first; None for an empty slot.
fn skinnable_leaders<F: FrameBuffer + ?Sized>(
    frame: &F,
    slot: SlotRect,
    matcher: &IconMatcher,
    source: &dyn SkinIconSource,
) -> Option<Vec<String>> {
    let sampler = FrameSlotSampler::new(frame, slot);
    if sampler.departure() < matcher.minimum_departure {
        return None;
    }

    let ranked = matcher.rank_sampler(&sampler, None, true);
    Some(
        ranked
            .iter()
            .take(skin_hunt::TOP)
            .filter(|m| is_default_skin_category(&m.item.category) && source.has_skins(&m.item.short_name))
            .map(|m| m.item.short_name.clone())
            .collect(),
    )
}

/// Logs contradicted counts, and keeps the pixels behind them.
///
/// A rejection is expected whenever a stack genuinely changes, so this is not a fault signal. It
/// exists so `CountReader` can be recalibrated against real frames rather than reconstructions.
fn record_rejections<F: FrameBuffer + ?Sized>(
    frame: &F,
    rejections: &[CountRejection],
    now: SystemTime,
    last_capture: &mut Option<SystemTime>,
    data_directory: &Path,
) {
    for rejection in rejections {
        tracing::info!(
            "Count rejected in belt slot {}: holding {}, read {}",
            rejection.slot,
            describe(rejection.confirmed),
            rejection.seen.map_or_else(|| "nothing".to_owned(), describe)
        );
    }

    if !capture_due(*last_capture, now) {
        return;
    }
    *last_capture = Some(now);

    match frame_image::save(frame, &data_directory.join(frame_image::DIRECTORY_NAME)) {
        Ok(path) => tracing::info!(path = %path.display(), "Saved the rejecting frame"),
        // Diagnostics must never take the capture loop down with them.
        Err(error) => tracing::warn!(%error, "Could not save the rejecting frame"),
    }
}

/// A clock that went backwards reads as inside the interval, as the C# TimeSpan comparison did.
fn capture_due(last: Option<SystemTime>, now: SystemTime) -> bool {
    match last {
        None => true,
        Some(at) => now.duration_since(at).is_ok_and(|gap| gap >= REJECTION_CAPTURE),
    }
}

fn describe(count: SlotCount) -> String {
    if count.is_stack { format!("x{}", count.value) } else { count.value.to_string() }
}

/// `{0:P0}` of dpi / 96 as .NET's invariant culture writes it, e.g. "125 %".
fn percent(dpi: u32) -> String {
    format!("{} %", (f64::from(dpi) / f64::from(display_scaling::UNSCALED) * 100.0).round())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_described_as_the_hud_draws_them() {
        assert_eq!(describe(SlotCount::new(30, true)), "x30");
        assert_eq!(describe(SlotCount::new(7, false)), "7");
    }

    #[test]
    fn rejecting_frames_are_saved_at_most_once_a_minute() {
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);

        assert!(capture_due(None, at));
        assert!(!capture_due(Some(at), at + Duration::from_secs(59)));
        assert!(capture_due(Some(at), at + REJECTION_CAPTURE));
        assert!(!capture_due(Some(at), at - Duration::from_secs(1)));
    }

    #[test]
    fn scale_is_written_as_a_whole_percentage() {
        assert_eq!(percent(120), "125 %");
        assert_eq!(percent(144), "150 %");
    }
}
