//! Finding the Rust install and reading the files the agent needs from it.

mod key_bindings;
mod rust_install;
mod skin_definitions;
mod steam_locator;

pub use key_bindings::{DEFAULT_INVENTORY_KEY, KeyBindings};
pub use rust_install::RustInstall;
pub use skin_definitions::{SkinDefinition, SkinDefinitions};
pub use steam_locator::{SteamRustInstallLocator, steam_root};

pub trait RustInstallLocator: Send + Sync {
    /// `None` when Rust is not installed, or not installed anywhere this can see.
    fn locate(&self) -> Option<RustInstall>;
}
