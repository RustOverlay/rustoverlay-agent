use std::sync::Arc;

use super::ItemDefinition;

/// The winner, plus any items whose icons cannot be told apart from it.
///
/// `indistinguishable` is empty in the ordinary case. When it is not, the slot holds one of the
/// listed items and the icon alone cannot say which — quality tiers and ammo variants are the usual
/// cause. Consumers that must not be wrong should surface the whole set or nothing; `item` is only
/// the best-scoring member and picking it is a coin flip.
#[derive(Clone, Debug, PartialEq)]
pub struct IconMatch {
    pub item: Arc<ItemDefinition>,
    pub score: f64,
    pub indistinguishable: Vec<Arc<ItemDefinition>>,
}

impl IconMatch {
    pub fn new(item: Arc<ItemDefinition>, score: f64) -> Self {
        Self { item, score, indistinguishable: Vec::new() }
    }
}
