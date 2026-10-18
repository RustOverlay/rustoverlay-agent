//! Lines taken from a real keys.cfg, including the ones that break a naive parser.

use ro_core::install::KeyBindings;

#[test]
fn reads_the_inventory_key() {
    let binds = KeyBindings::parse(["bind space +jump", "bind tab inventory.toggle", "bind a +left"]);

    assert_eq!(binds.inventory_key(), "tab");
}

#[test]
fn a_command_that_merely_starts_the_same_is_not_a_match() {
    // Real config: Q ends in inventory.togglecrafting. A prefix match reports Q as the inventory
    // key, and the agent then watches a key the player uses for skins.
    let binds = KeyBindings::parse(["bind q +prevskin;+prevskin;inventory.togglecrafting", "bind tab inventory.toggle"]);

    assert_eq!(binds.inventory_key(), "tab");
}

#[test]
fn finds_a_command_anywhere_in_a_chain() {
    let binds = KeyBindings::parse(["bind f +focusmap;lighttoggle;inventory.toggle"]);

    assert_eq!(binds.inventory_key(), "f");
}

#[test]
fn a_semicolon_inside_quotes_does_not_split_the_chain() {
    let binds = KeyBindings::parse(["bind c chat.say \"/remove;now\";inventory.toggle"]);

    assert_eq!(binds.inventory_key(), "c");
    assert_eq!(binds.key_for("chat.say"), Some("c"));
}

#[test]
fn falls_back_to_the_games_own_default_when_unbound() {
    assert_eq!(KeyBindings::parse(["bind space +jump"]).inventory_key(), "tab");
    assert_eq!(KeyBindings::empty().inventory_key(), "tab");
}

#[test]
fn ignores_lines_that_are_not_binds() {
    let binds = KeyBindings::parse(["// a comment", "", "graphics.fov 90", "bind tab inventory.toggle"]);

    assert_eq!(binds.inventory_key(), "tab");
    assert_eq!(binds.key_for("graphics.fov"), None);
}
