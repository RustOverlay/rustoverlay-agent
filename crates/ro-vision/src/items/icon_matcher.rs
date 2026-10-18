use std::collections::HashSet;
use std::sync::Arc;

use indexmap::IndexMap;

use super::{IconFace, IconMatch, IconSignature, ItemDefinition, SlotBackground, SlotSampler};
use crate::dotnet_sort::{compare_f64, sort_by};

/// Every face the matcher can name, in insertion order.
///
/// Ordered because candidates are visited in this order and the ranking sort is unstable, so the
/// order decides exact ties exactly as the C# `Dictionary` enumeration did.
pub type Catalogue = IndexMap<IconFace, IconSignature>;

/// A candidate filter: only items it accepts are considered.
pub type Candidates<'a> = Option<&'a dyn Fn(&ItemDefinition) -> bool>;

/// A candidate mid-flight: an index into the catalogue and its score.
#[derive(Clone, Copy, Debug)]
struct Ranked {
    pub face: usize,
    pub score: f64,
}

/// Resolves a slot's contents to an item by comparing its icon against the catalogue.
///
/// The candidate set is narrowed by slot before any pixels are compared — an armour slot can only
/// hold one of ~165 wearables — which is both faster and materially more accurate. Keyed on
/// [`IconFace`], so one item may appear under several pictures; faces never escape, and two faces
/// of one item are one answer, not an ambiguity.
///
/// Every tunable is a public field defaulted by [`IconMatcher::new`]; docs/item-matching.md records
/// how each was measured.
pub struct IconMatcher {
    catalogue: Catalogue,
    /// Below this the best candidate is reported as no match rather than a guess.
    pub minimum_score: f64,
    /// The same gate for a winning skin face. A skin the player owns but has not cached loses to
    /// some other skin on every frame alike, which `SlotHistory` cannot catch; real skin reads
    /// score 0.82-0.94, so the stricter gate costs none of them. See research/matcher-accuracy.md.
    pub minimum_skin_score: f64,
    /// How far ahead of the first candidate outside the winner's look-alike set the winner must be.
    /// This, not the absolute score, separates a real item from noise.
    pub minimum_margin: f64,
    /// Correlation above which two catalogue icons are treated as one answer.
    pub look_alike_at: f64,
    /// A looser grouping tried only on slots that would otherwise abstain. None disables it.
    /// Applied to every slot it demotes more single answers into sets than it rescues.
    pub ambiguous_at: Option<f64>,
    /// Largest look-alike set that is still an answer. Never fires at the shipped thresholds; kept
    /// because with `mutual_look_alikes` it turns a threshold 0.03 from a cliff into a plateau.
    pub max_look_alikes: usize,
    /// Whether every look-alike must resemble every other, not just the winner. A degenerate
    /// leader (a flat `water`) resembles many unrelated items that do not resemble each other.
    pub mutual_look_alikes: bool,
    /// Least a slot drawn over the world may depart from its margin's prediction and still be an
    /// item. Over-world only: over flat panel grey a thin item dilutes below it.
    pub minimum_departure: f64,
    /// Whether slots drawn over the world are scored against a per-cell scene prediction rather
    /// than one averaged background colour.
    pub scene_composited: bool,
    /// How many leading candidates are re-scored at the refined alignment.
    pub shortlist: usize,
    /// How far the icon area may be nudged, in pixels, while searching for its alignment.
    pub alignment_search: i32,
    /// How many candidates survive the 8x8 pass and get scored at full size; 0 disables it. A
    /// share of the catalogue, because what the shortlist survives is proportional competition.
    /// Two fifths: a quarter lost reads on every corpus, and scoring every face let more
    /// look-alikes through than the coarse pass does. See research/matcher-accuracy.md.
    pub coarse_shortlist: usize,
}

impl IconMatcher {
    pub fn new(catalogue: Catalogue) -> Self {
        let coarse_shortlist = 64.max(catalogue.len() * 2 / 5);

        Self {
            catalogue,
            minimum_score: 0.65,
            minimum_skin_score: 0.70,
            minimum_margin: 0.10,
            look_alike_at: IconSignature::INDISTINGUISHABLE_AT,
            ambiguous_at: Some(0.75),
            max_look_alikes: 3,
            mutual_look_alikes: true,
            minimum_departure: 0.039,
            scene_composited: true,
            shortlist: 25,
            alignment_search: 2,
            coarse_shortlist,
        }
    }

    pub fn catalogue(&self) -> &Catalogue {
        &self.catalogue
    }

    /// Ranks a fixed signature against the catalogue, best first, each item named once.
    pub fn rank(&self, slot: &IconSignature, background: SlotBackground, candidates: Candidates, scene: Option<&[f32]>) -> Vec<IconMatch> {
        self.project(&self.rank_faces(slot, background, candidates, scene))
    }

    /// Ranks at the nominal alignment, then re-scores the leading candidates at the one alignment
    /// that best fits the leader.
    ///
    /// One alignment for every candidate, chosen by the nominal leader alone. Letting each
    /// candidate keep its own best fit dissolves the margin; letting the shortlist compete for the
    /// alignment lets a wrong candidate buy its way to the front.
    pub fn rank_sampler(&self, slot: &dyn SlotSampler, candidates: Candidates, over_world: bool) -> Vec<IconMatch> {
        self.project(&self.rank_sampled_faces(slot, candidates, over_world))
    }

    /// None when nothing scores well enough, or when the top two are too close to call.
    ///
    /// The margin is measured over the best candidate that is not a look-alike of the winner:
    /// ranking `ammo.rifle` 0.001 above `ammo.snowballgun` says nothing about whether the slot is
    /// empty. Look-alikes travel in the result instead of suppressing it.
    pub fn identify(&self, slot: &IconSignature, background: SlotBackground, candidates: Candidates) -> Option<IconMatch> {
        self.decide(&self.rank_faces(slot, background, candidates, None))
    }

    /// As [`Self::identify`], re-cropping the slot to search its alignment. `over_world` marks a
    /// slot drawn over the scene rather than the panel — the belt with the inventory shut — and
    /// enables `minimum_departure`, which only holds there.
    ///
    /// Emptiness is settled before anything is ranked: the gates in `decide` judge a slot against
    /// the candidate that won it, the wrong question for a slot showing landscape.
    pub fn identify_sampler(&self, slot: &dyn SlotSampler, candidates: Candidates, over_world: bool) -> Option<IconMatch> {
        if over_world && slot.departure() < self.minimum_departure {
            return None;
        }

        self.decide(&self.rank_sampled_faces(slot, candidates, over_world))
    }

    fn rank_faces(&self, slot: &IconSignature, background: SlotBackground, candidates: Candidates, scene: Option<&[f32]>) -> Vec<Ranked> {
        let coarse = self.coarse_shortlist > 0;

        // Scored once, not in the sort comparator (n log n rescores); coarse stays on the flat background.
        let mut considered: Vec<Ranked> = self
            .catalogue
            .iter()
            .enumerate()
            .filter(|(_, (face, _))| candidates.is_none_or(|accept| accept(&face.item)))
            .map(|(face, (_, signature))| Ranked {
                face,
                score: if coarse { slot.coarse_similarity_to(signature, background) } else { 0.0 },
            })
            .collect();

        if coarse && considered.len() > self.coarse_shortlist {
            sort_by(&mut considered, |a, b| compare_f64(b.score, a.score));
            considered.truncate(self.coarse_shortlist);
        }

        for ranked in &mut considered {
            ranked.score = similarity(slot, self.signature(ranked.face), background, scene);
        }

        sort_by(&mut considered, |a, b| compare_f64(b.score, a.score));
        considered
    }

    fn rank_sampled_faces(&self, slot: &dyn SlotSampler, candidates: Candidates, over_world: bool) -> Vec<Ranked> {
        let background = slot.background();
        let scene = if self.scene_composited && over_world { slot.scene() } else { None };
        let nominal = slot.at(0, 0, 0);
        let ranked = self.rank_faces(&nominal, background, candidates, scene);
        let Some(first) = ranked.first() else { return ranked };

        let leader = self.signature(first.face);
        let mut aligned = nominal;
        let mut best_fit = first.score;
        let mut moved = false;

        // Offset first, then size at the winning offset: 27 re-crops instead of 75, scoring identically.
        let (mut best_x, mut best_y) = (0, 0);
        let search = self.alignment_search;

        for dy in -search..=search {
            for dx in -search..=search {
                if dx == 0 && dy == 0 {
                    continue;
                }

                let probe = slot.at(dx, dy, 0);
                let fit = similarity(&probe, leader, background, scene);
                if fit > best_fit {
                    (best_fit, aligned, moved, best_x, best_y) = (fit, probe, true, dx, dy);
                }
            }
        }

        for d_size in (2..=2 * search).step_by(2) {
            let probe = slot.at(best_x, best_y, d_size);
            let fit = similarity(&probe, leader, background, scene);
            if fit > best_fit {
                (best_fit, aligned, moved) = (fit, probe, true);
            }
        }

        if !moved {
            return ranked;
        }

        let mut shortlist: Vec<Ranked> = ranked
            .iter()
            .take(self.shortlist)
            .map(|m| Ranked { face: m.face, score: similarity(&aligned, self.signature(m.face), background, scene) })
            .collect();

        sort_by(&mut shortlist, |a, b| compare_f64(b.score, a.score));
        shortlist
    }

    /// Faces down to the items they name, keeping the best-scoring face of each — a skinned
    /// catalogue would otherwise report the same item several times over.
    fn project(&self, ranked: &[Ranked]) -> Vec<IconMatch> {
        let mut seen = HashSet::new();

        ranked
            .iter()
            .filter(|r| seen.insert(&self.face(r.face).item))
            .map(|r| IconMatch::new(Arc::clone(&self.face(r.face).item), r.score))
            .collect()
    }

    fn score_gate(&self, face: usize) -> f64 {
        if self.face(face).is_skin() { self.minimum_score.max(self.minimum_skin_score) } else { self.minimum_score }
    }

    fn decide(&self, ranked: &[Ranked]) -> Option<IconMatch> {
        self.decide_at(ranked, self.look_alike_at)
            .or_else(|| self.ambiguous_at.and_then(|loose| self.decide_at(ranked, loose)))
    }

    /// A candidate naming the item that already won is another picture of it — a skin — and is
    /// skipped outright: neither a rival to take the margin over nor an ambiguity to report.
    /// Counting them put every heavily skinned item over `max_look_alikes`.
    fn decide_at(&self, ranked: &[Ranked], look_alike_at: f64) -> Option<IconMatch> {
        let first = ranked.first().filter(|r| r.score >= self.score_gate(r.face))?;
        let winner_item = &self.face(first.face).item;
        let winner = self.signature(first.face);
        let mut look_alikes: Vec<usize> = Vec::new();
        let mut runner_up = f64::NEG_INFINITY;

        for r in &ranked[1..] {
            let item = &self.face(r.face).item;
            if item == winner_item {
                continue;
            }

            let candidate = self.signature(r.face);

            if winner.is_indistinguishable_from(candidate, look_alike_at)
                && (!self.mutual_look_alikes
                    || look_alikes.iter().all(|&a| candidate.is_indistinguishable_from(self.signature(a), look_alike_at)))
            {
                if look_alikes.iter().all(|&a| self.face(a).item != *item) {
                    look_alikes.push(r.face);
                }
                continue;
            }

            runner_up = r.score;
            break;
        }

        if look_alikes.len() + 1 > self.max_look_alikes || first.score - runner_up < self.minimum_margin {
            return None;
        }

        Some(IconMatch {
            item: Arc::clone(winner_item),
            score: first.score,
            indistinguishable: look_alikes.iter().map(|&a| Arc::clone(&self.face(a).item)).collect(),
        })
    }

    fn face(&self, index: usize) -> &IconFace {
        self.catalogue.get_index(index).expect("ranked index is in the catalogue").0
    }

    fn signature(&self, index: usize) -> &IconSignature {
        self.catalogue.get_index(index).expect("ranked index is in the catalogue").1
    }
}

fn similarity(slot: &IconSignature, candidate: &IconSignature, background: SlotBackground, scene: Option<&[f32]>) -> f64 {
    match scene {
        None => slot.similarity_to_composited(candidate, background),
        Some(scene) => slot.similarity_to_scene(candidate, background, scene),
    }
}
