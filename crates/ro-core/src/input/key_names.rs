//! The keys the agent can watch, by Rust's own lowercase name and by Windows virtual key code.
//!
//! One table in both directions on purpose. Settings binds by capturing a keystroke, which arrives
//! as a virtual key and has to be stored as a name, so a separate reverse map would be free to
//! drift until a key the settings screen offered turned out to be one the watcher ignores.
//!
//! Only keys a player might plausibly bind are here. Anything else resolves to 0, which disables
//! `KeyWatcher` rather than making it guess, and cannot be bound from the UI at all.

use std::collections::HashMap;
use std::sync::LazyLock;

struct Entry {
    name: String,
    code: i32,
    label: String,
}

struct Tables {
    by_name: HashMap<String, i32>,
    /// First entry per code wins: it is the name a capture stores back.
    by_code: HashMap<i32, (String, String)>,
}

static TABLES: LazyLock<Tables> = LazyLock::new(|| {
    let mut by_name = HashMap::new();
    let mut by_code = HashMap::new();

    for entry in build() {
        by_name.insert(entry.name.clone(), entry.code);
        by_code.entry(entry.code).or_insert((entry.name, entry.label));
    }

    Tables { by_name, by_code }
});

/// The virtual key code for a Rust key name, or 0 when it is not one we watch.
pub fn code_of(name: &str) -> i32 {
    TABLES.by_name.get(&name.trim().to_lowercase()).copied().unwrap_or(0)
}

/// The Rust key name for a virtual key code, or `None` when it is not bindable.
pub fn name_of(code: i32) -> Option<&'static str> {
    TABLES.by_code.get(&code).map(|(name, _)| name.as_str())
}

/// How a bound key is written to the player, e.g. `leftalt` as "Left Alt".
pub fn label(name: &str) -> String {
    TABLES.by_code.get(&code_of(name)).map_or_else(|| name.to_owned(), |(_, label)| label.clone())
}

fn build() -> Vec<Entry> {
    // "return" trails "enter" deliberately: the first name for a code is the one stored back.
    let named: [(&str, i32, &str); 22] = [
        ("tab", 0x09, "Tab"),
        ("space", 0x20, "Space"),
        ("enter", 0x0D, "Enter"),
        ("return", 0x0D, "Enter"),
        ("escape", 0x1B, "Escape"),
        ("backspace", 0x08, "Backspace"),
        ("insert", 0x2D, "Insert"),
        ("delete", 0x2E, "Delete"),
        ("home", 0x24, "Home"),
        ("end", 0x23, "End"),
        ("pageup", 0x21, "Page Up"),
        ("pagedown", 0x22, "Page Down"),
        ("leftshift", 0xA0, "Left Shift"),
        ("rightshift", 0xA1, "Right Shift"),
        ("leftcontrol", 0xA2, "Left Ctrl"),
        ("rightcontrol", 0xA3, "Right Ctrl"),
        ("leftalt", 0xA4, "Left Alt"),
        ("rightalt", 0xA5, "Right Alt"),
        ("uparrow", 0x26, "Up"),
        ("downarrow", 0x28, "Down"),
        ("leftarrow", 0x25, "Left"),
        ("rightarrow", 0x27, "Right"),
    ];

    let entry = |name: String, code: i32, label: String| Entry { name, code, label };

    let named = named.into_iter().map(|(n, c, l)| entry(n.into(), c, l.into()));
    let letters = ('a'..='z').map(|c| entry(c.into(), c.to_ascii_uppercase() as i32, c.to_ascii_uppercase().into()));
    let digits = ('0'..='9').map(|c| entry(c.into(), c as i32, c.into()));
    let function = (1..=12).map(|n| entry(format!("f{n}"), 0x6F + n, format!("F{n}")));
    let keypad = (0..10).map(|n| entry(format!("keypad{n}"), 0x60 + n, format!("Numpad {n}")));

    named.chain(letters).chain(digits).chain(function).chain(keypad).collect()
}
