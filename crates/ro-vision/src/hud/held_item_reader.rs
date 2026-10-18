//! Which belt slot is currently held. Rust tints the active slot blue, so no game state is needed.
//!
//! Measured at 1440x1080: the selected slot averages (70,103,134) against (116,113,113) for the
//! same slot unselected. Decided by comparing slots against each other rather than by an absolute
//! threshold alone: in world view the slot panels are transparent, so a player looking at sky or
//! water puts blue behind every slot. The winner must be both blue in absolute terms and clearly
//! bluer than its neighbours.

use super::{FrameBuffer, InventoryGeometry, SlotRect};

/// Minimum blue-over-red for a slot to be considered tinted at all.
const MINIMUM_BLUE_BIAS: i32 = 25;

/// How far ahead of the runner-up, to survive a blue sky behind the whole belt.
const MINIMUM_LEAD: i32 = 15;

/// None when no slot is held, or when the belt is too uniformly blue to tell.
pub fn selected_slot<F: FrameBuffer + ?Sized>(frame: &F, geometry: &InventoryGeometry) -> Option<usize> {
    let bias: Vec<i32> = geometry.belt().iter().map(|&slot| blue_bias(frame, slot)).collect();

    // First of equals wins, as the C# strict `>` scan did.
    let best = bias.iter().enumerate().fold(0, |best, (i, &b)| if b > bias[best] { i } else { best });

    if bias[best] < MINIMUM_BLUE_BIAS {
        return None;
    }

    let runner_up = bias.iter().enumerate().filter(|(i, _)| *i != best).map(|(_, &b)| b).max().unwrap_or(i32::MIN);

    if bias[best] - runner_up < MINIMUM_LEAD { None } else { Some(best) }
}

/// Blueness of the slot's margin only, never its centre.
///
/// The highlight tints the slot background, so it reaches the edges. An item icon is inset and
/// leaves the margin alone. Averaging the whole slot conflates the two: a blue item scored 79
/// against a genuine selection's 66, so no threshold on the full-slot average separates them.
fn blue_bias<F: FrameBuffer + ?Sized>(frame: &F, slot: SlotRect) -> i32 {
    let inset = 2.max(slot.size / 6);
    let (mut blue, mut red, mut n) = (0i64, 0i64, 0i64);

    let x1 = slot.right().min(frame.width() - 1);
    let y1 = slot.bottom().min(frame.height() - 1);

    for y in 0.max(slot.y)..=y1 {
        for x in 0.max(slot.x)..=x1 {
            let in_margin = x - slot.x < inset
                || slot.right() - x < inset
                || y - slot.y < inset
                || slot.bottom() - y < inset;

            if !in_margin {
                continue;
            }

            let c = frame.pixel(x, y);
            blue += i64::from(c.b);
            red += i64::from(c.r);
            n += 1;
        }
    }

    if n == 0 { 0 } else { ((blue - red) / n) as i32 }
}
