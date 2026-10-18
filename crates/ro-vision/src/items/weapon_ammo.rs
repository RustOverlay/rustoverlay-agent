use crate::text::eq_ignore_case;

/// The rounds one gun fires, matched case-insensitively.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmmoSet(&'static [&'static str]);

impl AmmoSet {
    pub fn contains(&self, short_name: &str) -> bool {
        self.0.iter().any(|round| eq_ignore_case(round, short_name))
    }

    pub fn rounds(&self) -> &'static [&'static str] {
        self.0
    }
}

const RIFLE_556: AmmoSet = AmmoSet(&["ammo.rifle", "ammo.rifle.explosive", "ammo.rifle.hv", "ammo.rifle.incendiary"]);
const PISTOL: AmmoSet = AmmoSet(&["ammo.pistol", "ammo.pistol.fire", "ammo.pistol.hv"]);
const SHELL: AmmoSet = AmmoSet(&["ammo.handmade.shell", "ammo.shotgun", "ammo.shotgun.fire", "ammo.shotgun.slug"]);

/// Which ammunition each gun fires, so a spare-ammo count can be matched to what is held.
///
/// Hand-authored because nothing in the install carries it: the real link is
/// `BaseProjectile.primaryMagazine.ammoType`, in the compiled prefabs. Summing every `Ammunition`
/// item instead was rejected — it counts rockets and arrows as bullets. An unmapped weapon must
/// never fall back to a wider set; a plausible wrong number is worse on an overlay than a blank.
/// Authored from item names, so treat a surprising spare count as a suspect row here first.
pub struct WeaponAmmo;

impl WeaponAmmo {
    pub const BY_WEAPON: &[(&str, AmmoSet)] = &[
        ("blunderbuss", SHELL),
        ("hmlmg", RIFLE_556),
        ("krieg.shotgun", SHELL),
        ("lmg.m249", RIFLE_556),
        ("m16a2", RIFLE_556),
        ("minigun", RIFLE_556),
        ("pistol.eoka", SHELL),
        ("pistol.m92", PISTOL),
        ("pistol.prototype17", PISTOL),
        ("pistol.python", PISTOL),
        ("pistol.revolver", PISTOL),
        ("pistol.semiauto", PISTOL),
        ("pistol.semiauto.a.m15", PISTOL),
        ("revolver.hc", PISTOL),
        ("rifle.ak", RIFLE_556),
        ("rifle.ak.diver", RIFLE_556),
        ("rifle.ak.ice", RIFLE_556),
        ("rifle.ak.jungle", RIFLE_556),
        ("rifle.ak.med", RIFLE_556),
        ("rifle.bolt", RIFLE_556),
        ("rifle.l96", RIFLE_556),
        ("rifle.lr300", RIFLE_556),
        ("rifle.lr300.space", RIFLE_556),
        ("rifle.m39", RIFLE_556),
        ("rifle.semiauto", RIFLE_556),
        ("rifle.sks", RIFLE_556),
        ("shotgun.double", SHELL),
        ("shotgun.m4", SHELL),
        ("shotgun.pump", SHELL),
        ("shotgun.spas12", SHELL),
        ("shotgun.waterpipe", SHELL),
        ("smg.2", PISTOL),
        ("smg.mp5", PISTOL),
        ("smg.thompson", PISTOL),
        ("t1_smg", PISTOL),
    ];

    /// What the gun fires, or None when it is not a mapped firearm.
    pub fn for_weapon(weapon_short_name: &str) -> Option<AmmoSet> {
        Self::BY_WEAPON.iter().find(|(w, _)| eq_ignore_case(w, weapon_short_name)).map(|(_, ammo)| *ammo)
    }
}
