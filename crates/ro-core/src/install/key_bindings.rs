use std::collections::HashMap;
use std::io;
use std::path::Path;

use crate::invariant::read_all_lines;

/// Rust's own default, used when the file is missing or the bind is absent.
pub const DEFAULT_INVENTORY_KEY: &str = "tab";

/// The player's key binds, read from `cfg/keys.cfg`.
///
/// Read rather than assumed because the agent watches for the inventory key to catch a panel that
/// opens and closes between polls, and a player who has rebound it off Tab would otherwise never be
/// watched at all.
#[derive(Debug, Clone, Default)]
pub struct KeyBindings {
    by_command: HashMap<String, String>,
}

impl KeyBindings {
    pub fn empty() -> Self {
        Self::default()
    }

    /// The key bound to `inventory.toggle`.
    pub fn inventory_key(&self) -> &str {
        self.key_for("inventory.toggle").unwrap_or(DEFAULT_INVENTORY_KEY)
    }

    /// `None` when nothing is bound to the command.
    pub fn key_for(&self, command: &str) -> Option<&str> {
        self.by_command.get(command).map(String::as_str)
    }

    /// A missing file reads as no binds; an unreadable one is an error.
    pub fn load(path: &Path) -> io::Result<Self> {
        if !path.is_file() {
            return Ok(Self::empty());
        }
        read_all_lines(path).map(Self::parse)
    }

    /// Parses `bind <key> <command>[;<command>...]`.
    ///
    /// A bind's action is a chain, so every command in it has to be considered separately, and
    /// each compared on its first word rather than by prefix. Both matter in a real config: the
    /// sample this was written against binds `inventory.toggle` to Tab while Q ends in
    /// `inventory.togglecrafting`, which a prefix match would claim instead.
    pub fn parse<S: AsRef<str>>(lines: impl IntoIterator<Item = S>) -> Self {
        let mut bindings = Self::empty();

        for line in lines {
            let text = line.as_ref().trim();
            if !text.get(..5).is_some_and(|head| head.eq_ignore_ascii_case("bind ")) {
                continue;
            }

            let rest = text[5..].trim_start();
            let Some(split) = rest.find(' ').filter(|&s| s > 0) else { continue };

            let key = &rest[..split];

            for command in split_commands(&rest[split + 1..]) {
                // First bind wins, so an earlier line is not silently replaced by a later chain.
                if let Some(name) = command.split(' ').find(|word| !word.is_empty()) {
                    bindings.by_command.entry(name.to_owned()).or_insert_with(|| key.to_owned());
                }
            }
        }

        bindings
    }
}

/// Splits on ';', except inside quotes — `chat.say "a;b"` is one command.
fn split_commands(chain: &str) -> Vec<&str> {
    let mut commands = Vec::new();
    let mut start = 0;
    let mut quoted = false;

    for (i, c) in chain.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ';' if !quoted => {
                commands.push(chain[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }

    commands.push(chain[start..].trim());
    commands
}
