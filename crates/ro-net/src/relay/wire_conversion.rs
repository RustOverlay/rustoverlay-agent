//! Field-for-field conversion between the generated messages and `ro_vision::items::wire`, the
//! codegen-free mirror ro-vision's loadout readers take.

use ro_vision::items::wire;

use crate::proto;

impl From<proto::Slot> for wire::Slot {
    fn from(s: proto::Slot) -> Self {
        Self { index: s.index, item_id: s.item_id, count: s.count, is_stack: s.is_stack }
    }
}

impl From<wire::Slot> for proto::Slot {
    fn from(s: wire::Slot) -> Self {
        Self { index: s.index, item_id: s.item_id, count: s.count, is_stack: s.is_stack }
    }
}

impl From<proto::Container> for wire::Container {
    fn from(c: proto::Container) -> Self {
        Self { read_at_ms: c.read_at_ms, slots: c.slots.into_iter().map(Into::into).collect() }
    }
}

impl From<wire::Container> for proto::Container {
    fn from(c: wire::Container) -> Self {
        Self { read_at_ms: c.read_at_ms, slots: c.slots.into_iter().map(Into::into).collect() }
    }
}

impl From<proto::Loadout> for wire::Loadout {
    fn from(l: proto::Loadout) -> Self {
        Self {
            sampled_at_ms: l.sampled_at_ms,
            belt: l.belt.map(Into::into),
            main: l.main.map(Into::into),
            wear: l.wear.map(Into::into),
            backpack: l.backpack.map(Into::into),
            held_slot: l.held_slot,
        }
    }
}

impl From<wire::Loadout> for proto::Loadout {
    fn from(l: wire::Loadout) -> Self {
        Self {
            sampled_at_ms: l.sampled_at_ms,
            belt: l.belt.map(Into::into),
            main: l.main.map(Into::into),
            wear: l.wear.map(Into::into),
            backpack: l.backpack.map(Into::into),
            held_slot: l.held_slot,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loadout_survives_the_round_trip() {
        let original = proto::Loadout {
            sampled_at_ms: 42,
            belt: Some(proto::Container {
                read_at_ms: 41,
                slots: vec![proto::Slot { index: 2, item_id: vec![1, 2], count: 64, is_stack: true }],
            }),
            held_slot: Some(3),
            ..Default::default()
        };

        let mirrored: wire::Loadout = original.clone().into();
        assert_eq!(mirrored.belt.as_ref().unwrap().slots[0].item_id, vec![1, 2]);
        assert_eq!(proto::Loadout::from(mirrored), original);
    }
}
