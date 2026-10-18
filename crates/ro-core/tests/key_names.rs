//! The key table, which is only worth having because it reads both ways. Settings stores a name
//! and the watcher resolves a code, so the pair has to agree or a bind made in the UI is one the
//! overlay silently ignores.

use ro_core::input::key_names;

#[test]
fn every_code_that_names_a_key_resolves_back_to_that_same_code() {
    for code in 0..=0xFF {
        if let Some(name) = key_names::name_of(code) {
            assert_eq!(key_names::code_of(name), code, "{name}");
        }
    }
}

#[test]
fn names_resolve_to_the_codes_they_always_have() {
    for (name, code) in [
        ("tab", 0x09),
        ("leftalt", 0xA4),
        ("a", 0x41),
        ("0", 0x30),
        ("f4", 0x73),
        ("f10", 0x79),
        ("keypad0", 0x60),
        ("keypad9", 0x69),
    ] {
        assert_eq!(key_names::code_of(name), code, "{name}");
    }
}

#[test]
fn an_alias_resolves_but_is_not_what_a_capture_stores_back() {
    assert_eq!(key_names::code_of("enter"), key_names::code_of("return"));
    assert_eq!(key_names::name_of(key_names::code_of("return")), Some("enter"));
}

#[test]
fn whitespace_and_case_are_forgiven_because_the_name_may_be_hand_typed() {
    assert_eq!(key_names::code_of("leftalt"), key_names::code_of("  LeftAlt "));
}

#[test]
fn a_key_we_do_not_watch_resolves_to_nothing_rather_than_to_something_wrong() {
    assert_eq!(key_names::code_of("scrolllock"), 0);
    assert_eq!(key_names::code_of(""), 0);
    assert_eq!(key_names::name_of(0), None);
    assert_eq!(key_names::name_of(0x91), None);
}

#[test]
fn labels_are_written_the_way_a_player_would_read_them() {
    assert_eq!(key_names::label("leftalt"), "Left Alt");
    assert_eq!(key_names::label("keypad3"), "Numpad 3");
    assert_eq!(key_names::label("f4"), "F4");
    assert_eq!(key_names::label("c"), "C");
}

#[test]
fn an_unlabelled_name_is_shown_as_written_rather_than_as_a_blank_field() {
    assert_eq!(key_names::label("scrolllock"), "scrolllock");
}
