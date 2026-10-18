//! A cheap digest of a slot's pixels, used to tell whether it changed between frames.
//!
//! `InventoryReader` skips work when a slot has not changed, and `SlotHistory` refuses to count an
//! unchanged slot as fresh evidence. They must agree, so the digest lives here.

use super::{FrameBuffer, SlotRect};

/// Every `n`th pixel of a slot enters the digest, along each axis. A false miss needs two items to
/// agree on every sampled pixel, which no pair of icons does; a false hit only costs work.
const STRIDE: usize = 3;

const FNV_OFFSET: u64 = 14695981039346656037;
const FNV_PRIME: u64 = 1099511628211;

pub fn of<F: FrameBuffer + ?Sized>(frame: &F, slot: SlotRect) -> u64 {
    let x1 = slot.right().min(frame.width() - 1);
    let y1 = slot.bottom().min(frame.height() - 1);
    let mut hash = FNV_OFFSET;

    for y in (0.max(slot.y)..=y1).step_by(STRIDE) {
        for x in (0.max(slot.x)..=x1).step_by(STRIDE) {
            let c = frame.pixel(x, y);
            hash = (hash ^ u64::from(c.r)).wrapping_mul(FNV_PRIME);
            hash = (hash ^ u64::from(c.g)).wrapping_mul(FNV_PRIME);
            hash = (hash ^ u64::from(c.b)).wrapping_mul(FNV_PRIME);
        }
    }

    hash
}
