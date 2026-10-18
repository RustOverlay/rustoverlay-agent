#![allow(dead_code)]

pub mod relay;

use std::io;
use std::sync::Mutex;

use ro_net::Revision;
use ro_net::enrollment::{InstallToken, TokenStore};
use tokio::sync::watch;

/// The token store without DPAPI or a file, for anything that only cares what it holds.
#[derive(Default)]
pub struct MemoryTokenStore {
    stored: Mutex<Option<InstallToken>>,
    revision: Revision,
}

impl MemoryTokenStore {
    pub fn stored(&self) -> Option<InstallToken> {
        self.stored.lock().unwrap().clone()
    }
}

impl TokenStore for MemoryTokenStore {
    fn load(&self) -> Option<InstallToken> {
        self.stored()
    }

    fn save(&self, token: &InstallToken) -> io::Result<()> {
        *self.stored.lock().unwrap() = Some(token.clone());
        self.revision.bump();
        Ok(())
    }

    fn clear(&self) -> io::Result<()> {
        *self.stored.lock().unwrap() = None;
        self.revision.bump();
        Ok(())
    }

    fn changes(&self) -> watch::Receiver<u64> {
        self.revision.watch()
    }
}

pub fn hexay() -> InstallToken {
    InstallToken::new("tok_live", 76561198000000001, "Hexay")
}
