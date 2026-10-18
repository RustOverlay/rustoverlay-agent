//! The workshop `ItemType` labels that do not match a definition's own `name`.
//!
//! A skin manifest names the item it paints with a display label — "Metal Facemask", "AK47". 61 of
//! the 85 labels in circulation are a definition's name verbatim; these are the rest, mostly older
//! names Facepunch has since changed on the item but not in the workshop. It fails safe: a label
//! that stops resolving drops its skins, so those slots abstain rather than read wrong, and
//! `SkinCatalog::unmapped_labels` makes that visible.

use crate::text::eq_ignore_case;

pub const BY_SHORT_NAME: &[(&str, &str)] = &[
    ("AK47", "rifle.ak"),
    ("Balaclava", "mask.balaclava"),
    ("Bandana", "mask.bandana"),
    ("Bearskin Rug", "rug.bear"),
    ("Bolt Rifle", "rifle.bolt"),
    ("Burlap Pants", "burlap.trousers"),
    ("Collared Shirt", "shirt.collared"),
    ("Hide Shirt", "attire.hide.vest"),
    ("L96", "rifle.l96"),
    ("Long TShirt", "tshirt.long"),
    ("LR300", "rifle.lr300"),
    ("M39", "rifle.m39"),
    ("Miner Hat", "hat.miner"),
    ("Mp5", "smg.mp5"),
    ("Pick Axe", "pickaxe"),
    ("Python", "pistol.python"),
    ("Roadsign Gloves", "roadsign.gloves"),
    ("Roadsign Pants", "roadsign.kilt"),
    ("Roadsign Vest", "roadsign.jacket"),
    ("Stone Pick Axe", "stone.pickaxe"),
    ("Sword", "salvaged.sword"),
    ("Vagabond Jacket", "jacket"),
    ("Wooden Double Door", "door.double.hinged.wood"),
    ("Work Boots", "shoes.boots"),
];

/// The short name a workshop label stands for, case-insensitively, when the table knows it.
pub fn short_name_for_label(label: &str) -> Option<&'static str> {
    BY_SHORT_NAME.iter().find(|(l, _)| eq_ignore_case(l, label)).map(|(_, short)| *short)
}
