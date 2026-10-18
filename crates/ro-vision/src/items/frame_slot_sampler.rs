use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use super::slot_crop::crop;
use super::{IconSignature, SlotBackground, SlotSampler};
use crate::hud::{FrameBuffer, SlotRect};

const N: usize = IconSignature::SIZE;

/// Re-crops one slot of a captured frame so the matcher can search for the icon's true alignment.
///
/// Reads a [`FrameBuffer`] and nothing else, which is what lets a whole inventory be matched in
/// parallel: every sampler is independent. Signatures are cached per offset because the matcher
/// asks for the same handful repeatedly — once to fit the leader, then again to score the
/// shortlist at the winning offset.
pub struct FrameSlotSampler<'a, F: FrameBuffer + ?Sized> {
    frame: &'a F,
    icon: SlotRect,
    background: SlotBackground,
    departure: f64,
    scene: Option<Vec<f32>>,
    cache: RefCell<HashMap<(i32, i32, i32), Arc<IconSignature>>>,
}

impl<'a, F: FrameBuffer + ?Sized> FrameSlotSampler<'a, F> {
    pub fn new(frame: &'a F, slot: SlotRect) -> Self {
        Self::with_icon_area(frame, slot, slot.icon_area())
    }

    /// Overrides where the icon is taken to sit, so the icon model can be swept against a corpus
    /// without editing constants — the margin, the departure and the scene field all follow it.
    pub fn with_icon_area(frame: &'a F, slot: SlotRect, icon: SlotRect) -> Self {
        Self {
            frame,
            icon,
            background: Self::background_of(frame, slot, icon),
            departure: Self::departure_from(frame, slot, icon),
            scene: Self::scene_from(frame, slot, icon),
            cache: RefCell::default(),
        }
    }

    /// How far the icon area departs from what the margin around it predicts, as a mean absolute
    /// channel error over 0..1. Infinite when the slot has no margin to predict from.
    ///
    /// The empty-slot detector for a belt drawn over the world, where terrain has as much
    /// structure as any item. Interpolating across the icon area from the slot's own four margin
    /// edges predicts an empty slot well, because inside and outside are the same scene; an item is
    /// opaque and unrelated to what is behind it, so the interpolation misses. 14 of 21 inventions
    /// go; the 7 left are busy terrain. See docs/item-matching.md.
    pub fn departure_from(frame: &F, slot: SlotRect, icon: SlotRect) -> f64 {
        if !icon.lies_within(frame.width(), frame.height()) {
            return f64::INFINITY;
        }
        let Some(edges) = Edges::of(frame, slot, icon) else { return f64::INFINITY };

        let mut residual = 0.0;
        let mut n = 0u32;

        for row in 0..icon.size {
            for column in 0..icon.size {
                let pixel = frame.pixel(icon.x + column, icon.y + row);

                for (channel, was) in [pixel.r, pixel.g, pixel.b].into_iter().enumerate() {
                    residual += (f64::from(was) - edges.predict(icon.size, row, column, channel)).abs();
                    n += 1;
                }
            }
        }

        if n == 0 { f64::INFINITY } else { residual / f64::from(n) / 255.0 }
    }

    /// The scene the icon area is drawn over, predicted from the margin and sampled onto the
    /// signature grid. None when the slot has no margin to interpolate from.
    ///
    /// The per-cell counterpart of [`Self::background_of`]. Computed once at the nominal alignment
    /// and reused across the search: the field is smooth by construction, so a two pixel shift
    /// moves it far less than the icon it is subtracted from.
    pub fn scene_from(frame: &F, slot: SlotRect, icon: SlotRect) -> Option<Vec<f32>> {
        let edges = Edges::of(frame, slot, icon)?;
        let mut scene = vec![0f32; N * N * 3];
        let size = icon.size;
        let n = N as i32;

        for cy in 0..n {
            let row = (size - 1).min((cy * 2 + 1) * size / (2 * n));

            for cx in 0..n {
                let column = (size - 1).min((cx * 2 + 1) * size / (2 * n));
                let cell = ((cy * n + cx) * 3) as usize;

                for channel in 0..3 {
                    scene[cell + channel] = (edges.predict(size, row, column, channel) / 255.0) as f32;
                }
            }
        }

        Some(scene)
    }

    /// What the slot composites its icon over, measured from the margin outside the icon area —
    /// panel plus whatever the game draws behind it, and no item pixels.
    ///
    /// A mean, though the margin also carries the condition bar, the count and on the selected
    /// slot the highlight's edge. A per-channel median was measured as a wash; what costs reads on
    /// a highlighted slot is the correlation, not this estimate.
    pub fn background_of(frame: &F, slot: SlotRect, icon: SlotRect) -> SlotBackground {
        let (mut r, mut g, mut b) = (0.0, 0.0, 0.0);
        let mut n = 0u32;

        for y in 0.max(slot.y)..=(frame.height() - 1).min(slot.bottom()) {
            for x in 0.max(slot.x)..=(frame.width() - 1).min(slot.right()) {
                if x >= icon.x && x <= icon.right() && y >= icon.y && y <= icon.bottom() {
                    continue;
                }

                let c = frame.pixel(x, y);
                r += f64::from(c.r);
                g += f64::from(c.g);
                b += f64::from(c.b);
                n += 1;
            }
        }

        if n == 0 {
            return SlotBackground::default();
        }

        let n = f64::from(n);
        SlotBackground::new((r / n / 255.0) as f32, (g / n / 255.0) as f32, (b / n / 255.0) as f32)
    }
}

impl<F: FrameBuffer + ?Sized> SlotSampler for FrameSlotSampler<'_, F> {
    fn background(&self) -> SlotBackground {
        self.background
    }

    fn departure(&self) -> f64 {
        self.departure
    }

    fn scene(&self) -> Option<&[f32]> {
        self.scene.as_deref()
    }

    fn at(&self, dx: i32, dy: i32, d_size: i32) -> Arc<IconSignature> {
        let area = SlotRect::new(self.icon.x + dx, self.icon.y + dy, self.icon.size + d_size);
        let mut cache = self.cache.borrow_mut();

        Arc::clone(cache.entry((dx, dy, d_size)).or_insert_with(|| Arc::new(crop(self.frame, area))))
    }
}

/// Mean colour of each margin row and column, once rather than per pixel that reads it.
struct Edges {
    left: Vec<f64>,
    right: Vec<f64>,
    top: Vec<f64>,
    bottom: Vec<f64>,
}

impl Edges {
    fn of<F: FrameBuffer + ?Sized>(frame: &F, slot: SlotRect, icon: SlotRect) -> Option<Edges> {
        if icon.size < 2 || icon.x <= slot.x || icon.y <= slot.y {
            return None;
        }

        let len = icon.size as usize * 3;
        let mut edges = Edges { left: vec![0.0; len], right: vec![0.0; len], top: vec![0.0; len], bottom: vec![0.0; len] };

        for i in 0..icon.size {
            let at = i as usize * 3;
            mean(frame, slot.x, icon.y + i, icon.x - slot.x, 1, &mut edges.left[at..at + 3]);
            mean(frame, icon.right() + 1, icon.y + i, slot.right() - icon.right(), 1, &mut edges.right[at..at + 3]);
            mean(frame, icon.x + i, slot.y, 1, icon.y - slot.y, &mut edges.top[at..at + 3]);
            mean(frame, icon.x + i, icon.bottom() + 1, 1, slot.bottom() - icon.bottom(), &mut edges.bottom[at..at + 3]);
        }

        Some(edges)
    }

    fn predict(&self, size: i32, row: i32, column: i32, channel: usize) -> f64 {
        let u = f64::from(column) / f64::from(size - 1);
        let v = f64::from(row) / f64::from(size - 1);
        let (r, c) = (row as usize * 3 + channel, column as usize * 3 + channel);
        let horizontal = self.left[r] * (1.0 - u) + self.right[r] * u;
        let vertical = self.top[c] * (1.0 - v) + self.bottom[c] * v;

        (horizontal + vertical) / 2.0
    }
}

/// Mean colour of a rect into `into[0..3]`, left untouched when the rect is off the frame.
fn mean<F: FrameBuffer + ?Sized>(frame: &F, left: i32, top: i32, width: i32, height: i32, into: &mut [f64]) {
    let (mut r, mut g, mut b) = (0.0, 0.0, 0.0);
    let mut n = 0u32;

    for y in 0.max(top)..frame.height().min(top + height) {
        for x in 0.max(left)..frame.width().min(left + width) {
            let c = frame.pixel(x, y);
            r += f64::from(c.r);
            g += f64::from(c.g);
            b += f64::from(c.b);
            n += 1;
        }
    }

    if n == 0 {
        return;
    }

    let n = f64::from(n);
    into[0] = r / n;
    into[1] = g / n;
    into[2] = b / n;
}

