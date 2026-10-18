//! Which skin pictures are pictures of the whole item.

use std::collections::HashMap;
use std::sync::Arc;

use super::{Catalogue, IconSignature, ItemDefinition};

/// Least share of the item's own opaque area a skin's icon must cover to be admitted.
///
/// Some cached skins are not pictures of the item at all: one labelled Sheet Metal Door is two hinge
/// plates on a transparent canvas. Faces like that win fragments of real items and empty slots, and
/// were where every confirmed wrong read from the Attire and Construction skins came from; at 0.6
/// all of those go and no correct read is lost. See research/matcher-accuracy.md.
pub const MINIMUM_COVERAGE: f32 = 0.6;

/// Drops every skin face in `skins` that covers less than [`MINIMUM_COVERAGE`] of its item's own
/// icon in `vanilla`, or whose item has no icon there to compare with. Returns how many went.
pub fn retain_covering_skins(skins: &mut Catalogue, vanilla: &Catalogue) -> usize {
    let own: HashMap<&Arc<ItemDefinition>, f32> =
        vanilla.iter().filter(|(face, _)| !face.is_skin()).map(|(face, signature)| (&face.item, mass(signature))).collect();

    let before = skins.len();
    skins.retain(|face, signature| {
        !face.is_skin() || own.get(&face.item).is_some_and(|&item| mass(signature) >= MINIMUM_COVERAGE * item)
    });
    before - skins.len()
}

/// [`retain_covering_skins`] over one catalogue holding both kinds of face; skins keep their order
/// after the item faces. Returns the catalogue and how many skins went.
pub fn without_partial_skins(catalogue: Catalogue) -> (Catalogue, usize) {
    let (mut skins, mut kept): (Catalogue, Catalogue) = catalogue.into_iter().partition(|(face, _)| face.is_skin());
    let dropped = retain_covering_skins(&mut skins, &kept);
    kept.extend(skins);
    (kept, dropped)
}

fn mass(signature: &IconSignature) -> f32 {
    signature.alpha().iter().sum()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::items::IconFace;

    const N: usize = IconSignature::SIZE;

    fn item(short_name: &str) -> Arc<ItemDefinition> {
        Arc::new(ItemDefinition {
            item_id: 0,
            short_name: short_name.into(),
            name: short_name.into(),
            category: "Construction".into(),
            is_wearable: false,
            is_holdable: false,
            has_condition_bar: false,
            stack_size: 1,
            icon_path: Some(PathBuf::from(format!("{short_name}.png"))),
        })
    }

    /// Opaque over the first `cells` cells, transparent after.
    fn covering(cells: usize) -> IconSignature {
        IconSignature::new(vec![0.5; N * N * 3], (0..N * N).map(|c| if c < cells { 1.0 } else { 0.0 }).collect())
    }

    fn skin(item: &Arc<ItemDefinition>, name: &str) -> IconFace {
        IconFace::new(Arc::clone(item), Some(PathBuf::from(name)))
    }

    #[test]
    fn a_skin_must_cover_most_of_its_items_own_icon() {
        let door = item("door.hinged.metal");
        let vanilla = Catalogue::from([(IconFace::from(Arc::clone(&door)), covering(500))]);
        let mut skins = Catalogue::from([(skin(&door, "whole.png"), covering(400)), (skin(&door, "hinges.png"), covering(200))]);

        assert_eq!(retain_covering_skins(&mut skins, &vanilla), 1);
        assert_eq!(skins.keys().map(|f| f.icon_path.clone().unwrap()).collect::<Vec<_>>(), [PathBuf::from("whole.png")]);
    }

    #[test]
    fn a_skin_with_no_vanilla_icon_to_compare_is_dropped_and_vanilla_faces_are_kept() {
        let (door, ak) = (item("door.hinged.metal"), item("rifle.ak"));
        let vanilla = Catalogue::from([(IconFace::from(Arc::clone(&door)), covering(500))]);
        let mut mixed = Catalogue::from([(IconFace::from(Arc::clone(&ak)), covering(10)), (skin(&ak, "ak.png"), covering(900))]);

        assert_eq!(retain_covering_skins(&mut mixed, &vanilla), 1);
        assert!(mixed.keys().all(|f| !f.is_skin()));
    }
}
