use super::moments::{self, Moments};
use super::SlotBackground;

/// A cell counts as icon rather than background above this coverage.
const OPAQUE_THRESHOLD: f32 = 0.6;

/// Compositing predicts what a partly transparent cell actually shows, so such cells become usable
/// evidence instead of noise and the mask can be far more permissive.
const COMPOSITED_THRESHOLD: f32 = 0.1;

/// Least share of the icon's predicted structure a slot must actually show — the empty-slot
/// detector that works. See [`predicts_the_structure`].
const MINIMUM_STRUCTURE_RATIO: f64 = 0.35;

/// Channel spread above which a background is treated as coloured rather than neutral. Panel greys
/// measure 0.02-0.03 and the blue fill behind the selected belt slot 0.42.
const CHROMATIC_AT: f32 = 0.15;

const SAMPLES: usize = IconSignature::SIZE * IconSignature::SIZE * 3;

/// A downscaled, alpha-masked fingerprint of an icon, cheap enough to hold for the whole catalogue.
///
/// Comparison happens only where the icon is opaque. In a slot the icon is composited over the
/// slot background, so edge and transparent pixels carry the background rather than the item —
/// masking them out is what makes a match independent of what is drawn behind.
#[derive(Clone, Debug, PartialEq)]
pub struct IconSignature {
    rgb: Vec<f32>,
    alpha: Vec<f32>,
    coarse_rgb: Vec<f32>,
    coarse_alpha: Vec<f32>,
    opaque_cells: usize,
    /// Cells at or above `COMPOSITED_THRESHOLD`, ascending: the candidate-side mask every
    /// composited comparison walks, precomputed because it never changes per comparison.
    composited: Vec<u16>,
    coarse_composited: Vec<u16>,
}

impl IconSignature {
    /// 32 rather than 16 because thin icons — bows, arrows, rods — cover too few cells at 16 to
    /// correlate at all, and scored 0 against themselves.
    pub const SIZE: usize = 32;

    /// Side of the cheap fingerprint used to shortlist the catalogue before scoring it properly.
    /// A pass at 8x8 is 16x cheaper than 32x32 and only has to keep the right item in the shortlist.
    pub const COARSE_SIZE: usize = 8;

    /// Above this, two catalogue icons cannot be told apart by correlation alone. The default
    /// only; `IconMatcher::look_alike_at` is what callers actually set.
    pub const INDISTINGUISHABLE_AT: f64 = 0.90;

    /// Panics unless `rgb` is SIZE x SIZE x 3 and `alpha` SIZE x SIZE, as the C# constructor threw.
    pub fn new(rgb: Vec<f32>, alpha: Vec<f32>) -> Self {
        assert_eq!(rgb.len(), SAMPLES, "expected {0}x{0} RGB", Self::SIZE);
        assert_eq!(alpha.len(), Self::SIZE * Self::SIZE, "expected {0}x{0} alpha", Self::SIZE);

        let opaque_cells = alpha.iter().filter(|&&a| a >= OPAQUE_THRESHOLD).count();
        let (coarse_rgb, coarse_alpha) = downsample(&rgb, &alpha);
        let composited = cells_at_least(&alpha, COMPOSITED_THRESHOLD);
        let coarse_composited = cells_at_least(&coarse_alpha, COMPOSITED_THRESHOLD);

        Self { rgb, alpha, coarse_rgb, coarse_alpha, opaque_cells, composited, coarse_composited }
    }

    pub fn opaque_cells(&self) -> usize {
        self.opaque_cells
    }

    pub fn rgb(&self) -> &[f32] {
        &self.rgb
    }

    pub fn alpha(&self) -> &[f32] {
        &self.alpha
    }

    /// The same comparison at `COARSE_SIZE`, for shortlisting only. Deliberately ungated: it is a
    /// filter, and gating twice would only add a way to lose a real item before anything has
    /// looked at it closely.
    pub fn coarse_similarity_to(&self, other: &IconSignature, background: SlotBackground) -> f64 {
        const CELLS: usize = IconSignature::COARSE_SIZE * IconSignature::COARSE_SIZE;
        let (mut mine, mut theirs) = ([0f32; CELLS * 3], [0f32; CELLS * 3]);

        let back = background.channels();
        let n = gather(other.coarse_composited.iter().map(|&cell| usize::from(cell)), &mut mine, &mut theirs, |cell| {
            let (a, icon) = (other.coarse_alpha[cell], texel(&other.coarse_rgb, cell));
            (texel(&self.coarse_rgb, cell), std::array::from_fn(|c| a * icon[c] + (1.0 - a) * back[c]))
        });

        if n < 12 { 1.0 } else { moments::correlation(&mine[..n], &theirs[..n], false) }
    }

    /// Zero-mean normalised correlation over cells opaque in both signatures. Returns 0 when they
    /// barely overlap.
    ///
    /// Correlating each channel separately and averaging was tried and was measurably worse:
    /// normalising channels independently discards colour, and the AK fell from rank 1 to 25.
    pub fn similarity_to(&self, other: &IconSignature) -> f64 {
        // Too little shared silhouette to judge; treat as no match rather than a lucky score.
        self.opaque_correlation(other).unwrap_or(0.0)
    }

    /// As [`Self::similarity_to`], but composites the candidate over the slot's measured background
    /// first, so the comparison predicts what the slot actually shows:
    /// `alpha * icon + (1 - alpha) * background`. That lets the mask drop from 0.6 coverage to 0.1, and is what keeps an empty
    /// slot from clearing the gate at a 0.10 margin. See docs/item-matching.md.
    pub fn similarity_to_composited(&self, other: &IconSignature, background: SlotBackground) -> f64 {
        let (mut mine, mut theirs) = ([0f32; SAMPLES], [0f32; SAMPLES]);

        let back = background.channels();
        let n = gather(other.composited_cells(), &mut mine, &mut theirs, |cell| {
            let (a, icon) = (other.alpha[cell], texel(&other.rgb, cell));
            (texel(&self.rgb, cell), std::array::from_fn(|c| a * icon[c] + (1.0 - a) * back[c]))
        });

        if n < 24 {
            return 0.0;
        }

        gated(moments::measure(&mine[..n], &theirs[..n], is_chromatic(background)))
    }

    /// As [`Self::similarity_to_composited`], but against a per-cell prediction of the scene, and
    /// correlating what each side adds to it: `slot - scene` against `alpha * (icon - scene)`.
    ///
    /// Both sides contain the scene, and shared structure is covariance that exists whichever
    /// candidate is scored. Subtracting it removes it by construction; an empty slot adds nothing,
    /// so its residual is noise about zero and the contrast gate rejects it without a fitted
    /// threshold.
    pub fn similarity_to_scene(&self, other: &IconSignature, background: SlotBackground, scene: &[f32]) -> f64 {
        let (mut mine, mut theirs) = ([0f32; SAMPLES], [0f32; SAMPLES]);

        let n = gather(other.composited_cells(), &mut mine, &mut theirs, |cell| {
            let (a, icon, slot, behind) = (other.alpha[cell], texel(&other.rgb, cell), texel(&self.rgb, cell), texel(scene, cell));
            (std::array::from_fn(|c| slot[c] - behind[c]), std::array::from_fn(|c| a * (icon[c] - behind[c])))
        });

        if n < 24 {
            return 0.0;
        }

        gated(moments::measure(&mine[..n], &theirs[..n], is_chromatic(background)))
    }

    /// Whether two catalogue icons are too alike to separate — quality tiers, ammo variants, the
    /// same silhouette in another colour. Masks on both alphas, since both sides have one.
    pub fn is_indistinguishable_from(&self, other: &IconSignature, at: f64) -> bool {
        self.opaque_correlation(other).is_some_and(|score| score >= at)
    }

    /// Correlation over cells opaque in both, or None when fewer than 24 samples overlap.
    fn opaque_correlation(&self, other: &IconSignature) -> Option<f64> {
        let (mut mine, mut theirs) = ([0f32; SAMPLES], [0f32; SAMPLES]);

        let n = gather(
            (0..Self::SIZE * Self::SIZE).filter(|&cell| self.alpha[cell] >= OPAQUE_THRESHOLD && other.alpha[cell] >= OPAQUE_THRESHOLD),
            &mut mine,
            &mut theirs,
            |cell| (texel(&self.rgb, cell), texel(&other.rgb, cell)),
        );

        (n >= 24).then(|| moments::correlation(&mine[..n], &theirs[..n], true))
    }
}

impl IconSignature {
    fn composited_cells(&self) -> impl Iterator<Item = usize> + '_ {
        self.composited.iter().map(|&cell| usize::from(cell))
    }
}

/// Copies each cell's three channel pairs into the front of `mine`/`theirs`, in `cells` order.
/// Per cell rather than per channel, so each array is bounds-checked once a cell, not once a sample.
fn gather(cells: impl Iterator<Item = usize>, mine: &mut [f32], theirs: &mut [f32], pair: impl Fn(usize) -> ([f32; 3], [f32; 3])) -> usize {
    let mut n = 0;
    for cell in cells {
        let (m, t) = pair(cell);
        mine[n..n + 3].copy_from_slice(&m);
        theirs[n..n + 3].copy_from_slice(&t);
        n += 3;
    }
    n
}

fn texel(values: &[f32], cell: usize) -> [f32; 3] {
    let at = &values[cell * 3..cell * 3 + 3];
    [at[0], at[1], at[2]]
}

fn cells_at_least(alpha: &[f32], threshold: f32) -> Vec<u16> {
    (0..alpha.len()).filter(|&cell| alpha[cell] >= threshold).map(|cell| cell as u16).collect()
}

fn gated(m: Moments) -> f64 {
    if !predicts_the_structure(&m) || !m.has_contrast() { 0.0 } else { m.correlation() }
}

/// Whether the slot carries as much structure as the composited icon says it should.
///
/// Correlation is scale invariant, which lets it survive the game dimming an icon — and also lets
/// a nearly flat slot correlate perfectly with anything once its noise is amplified. This is the
/// amount that had to be amplified. Over 332 labelled slots correct reads sit at 0.58-1.02 and the
/// three confidently wrong ones at 0.12-0.19; 0.35 sits in a 3x gap.
fn predicts_the_structure(m: &Moments) -> bool {
    m.spread_a() >= MINIMUM_STRUCTURE_RATIO * m.spread_b()
}

fn is_chromatic(background: SlotBackground) -> bool {
    let SlotBackground { r, g, b } = background;
    r.max(g.max(b)) - r.min(g.min(b)) >= CHROMATIC_AT
}

/// Box average down to `COARSE_SIZE`, alpha weighted so transparent cells do not tint it.
fn downsample(rgb: &[f32], alpha: &[f32]) -> (Vec<f32>, Vec<f32>) {
    const SIZE: usize = IconSignature::SIZE;
    const COARSE: usize = IconSignature::COARSE_SIZE;
    const BLOCK: usize = SIZE / COARSE;

    let mut coarse_rgb = vec![0f32; COARSE * COARSE * 3];
    let mut coarse_alpha = vec![0f32; COARSE * COARSE];

    for cy in 0..COARSE {
        for cx in 0..COARSE {
            let mut weight = 0f32;
            let mut sum = [0f32; 3];

            for y in cy * BLOCK..(cy + 1) * BLOCK {
                for x in cx * BLOCK..(cx + 1) * BLOCK {
                    let cell = y * SIZE + x;
                    weight += alpha[cell];
                    for c in 0..3 {
                        sum[c] += alpha[cell] * rgb[cell * 3 + c];
                    }
                }
            }

            let coarse = cy * COARSE + cx;
            coarse_alpha[coarse] = weight / (BLOCK * BLOCK) as f32;
            for c in 0..3 {
                coarse_rgb[coarse * 3 + c] = if weight <= 0.0 { 0.0 } else { sum[c] / weight };
            }
        }
    }

    (coarse_rgb, coarse_alpha)
}
