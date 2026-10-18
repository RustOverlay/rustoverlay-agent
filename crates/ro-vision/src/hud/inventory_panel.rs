//! Whether Rust is drawing the inventory panel.
//!
//! Load-bearing rather than an optimisation. The matcher's gates were calibrated on slots the game
//! was actually drawing; pointed at world content in the same coordinates they confabulate — 23
//! invented items across six world-view captures, up to 31 in a single frame. The belt is exempt,
//! being drawn in world view too. See docs/item-matching.md, including the three approaches that
//! failed.

use super::{luminance, FrameBuffer, InventoryGeometry};

/// Share of slot gaps that must read darker than both slots flanking them. Over fifteen labelled
/// frames every open one polls 1.00 and every shut one 0.08-0.26.
const MINIMUM_AGREEMENT: f64 = 0.6;

/// Slot fraction sampled either side of a gap. Inside the icon's 10% inset.
const EDGE_BAND: f64 = 1.0 / 12.0;

/// Luminance levels, of 255, by which a gap must beat its neighbours to count as darker.
///
/// Gotcha: guards floating point, not contrast. On a perfectly flat region the gap and edge means
/// are equal in principle and differ in the last bits of the accumulation; a strict comparison
/// turns that noise into votes and calls a blank screen an open panel.
const MINIMUM_STEP: f64 = 1.0;

/// True when the panel's slot lattice is present at the coordinates the geometry predicts.
///
/// A positive signature, not a statistic: every measure of what the world looks like has a scene
/// that defeats it, but the world cannot imitate an edge at exactly the predicted coordinate. Each
/// gap between adjacent slots is asked whether it is darker than the two slots beside it, and the
/// votes are counted. Agreement is near total or near absent; there is no middle.
pub fn is_open<F: FrameBuffer + ?Sized>(frame: &F, geometry: &InventoryGeometry) -> bool {
    const COLUMNS: usize = InventoryGeometry::MAIN_COLUMNS;
    let main = geometry.main();
    let (mut votes, mut polls) = (0u32, 0u32);

    for row in 0..InventoryGeometry::MAIN_ROWS {
        for col in 0..COLUMNS {
            let here = main[row * COLUMNS + col];
            let edge = 2.max((f64::from(here.size) * EDGE_BAND) as i32);

            if col + 1 < COLUMNS {
                let east = main[row * COLUMNS + col + 1];
                poll(
                    frame,
                    &mut votes,
                    &mut polls,
                    Area { left: here.right() + 1, top: here.y, width: east.x - here.right() - 1, height: here.size },
                    Area { left: here.right() - edge, top: here.y, width: edge, height: here.size },
                    Area { left: east.x, top: east.y, width: edge, height: here.size },
                );
            }

            if row + 1 < InventoryGeometry::MAIN_ROWS {
                let south = main[(row + 1) * COLUMNS + col];
                poll(
                    frame,
                    &mut votes,
                    &mut polls,
                    Area { left: here.x, top: here.bottom() + 1, width: here.size, height: south.y - here.bottom() - 1 },
                    Area { left: here.x, top: here.bottom() - edge, width: here.size, height: edge },
                    Area { left: south.x, top: south.y, width: here.size, height: edge },
                );
            }
        }
    }

    polls > 0 && f64::from(votes) / f64::from(polls) >= MINIMUM_AGREEMENT
}

#[derive(Clone, Copy)]
struct Area {
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

/// One gap against the two slots flanking it, both sampled the same size.
fn poll<F: FrameBuffer + ?Sized>(frame: &F, votes: &mut u32, polls: &mut u32, gap: Area, before: Area, after: Area) {
    if gap.width < 2 || gap.height < 2 {
        return;
    }

    let level = mean(frame, gap);
    *polls += 1;

    if level + MINIMUM_STEP < mean(frame, before) && level + MINIMUM_STEP < mean(frame, after) {
        *votes += 1;
    }
}

fn mean<F: FrameBuffer + ?Sized>(frame: &F, area: Area) -> f64 {
    let mut sum = 0.0;
    let mut n = 0u32;

    for y in 0.max(area.top)..frame.height().min(area.top + area.height) {
        for x in 0.max(area.left)..frame.width().min(area.left + area.width) {
            sum += luminance(frame.pixel(x, y));
            n += 1;
        }
    }

    if n == 0 { f64::MAX } else { sum / f64::from(n) }
}
