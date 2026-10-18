use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use tokio::sync::watch;
use windows::core::PCWSTR;

use super::{InstallToken, TokenStore};
use crate::Revision;

/// Not a secret. DPAPI mixes it into the key, so ciphertext lifted from this file cannot be fed to
/// another application's unprotect call under the same user. Must match the C# agent's, or tokens it
/// wrote stop decrypting.
const ENTROPY: &[u8] = b"RustOverlay.InstallToken.v1";

/// Stores the install token encrypted with DPAPI under the current user. Another account on the
/// same machine cannot decrypt it, and neither can the file copied to another machine.
pub struct DpapiTokenStore {
    path: PathBuf,
    revision: Revision,
}

impl DpapiTokenStore {
    /// `file_name` comes from [`super::BackendOptions::file_name`]; a token is only good at the backend that minted it.
    pub fn new(directory: &Path, file_name: &str) -> io::Result<Self> {
        fs::create_dir_all(directory)?;
        Ok(Self { path: directory.join(file_name), revision: Revision::default() })
    }

    fn read(&self) -> Result<InstallToken, String> {
        let cipher = fs::read(&self.path).map_err(|e| e.to_string())?;
        let plain = unprotect(&cipher).map_err(|e| e.to_string())?;
        serde_json::from_slice(&plain).map_err(|e| e.to_string())
    }
}

impl TokenStore for DpapiTokenStore {
    fn load(&self) -> Option<InstallToken> {
        if !self.path.is_file() {
            return None;
        }

        // A restored profile or a copied file decrypts to nothing. Treat it as unenrolled rather
        // than fatal: the user can sign in again, which is the only fix anyway.
        // An empty token (`{}` parses) would be sent as a blank bearer, refused, and read as a sign-out.
        self.read()
            .and_then(|t| if t.token.is_empty() { Err("it holds no token".into()) } else { Ok(t) })
            .inspect_err(|e| {
                tracing::warn!(error = %e, "Stored install token is unreadable; re-enrollment required")
            })
            .ok()
    }

    fn save(&self, token: &InstallToken) -> io::Result<()> {
        let plain = serde_json::to_vec(token)?;
        crate::atomic_file::write(&self.path, &protect(&plain)?)?;
        self.revision.bump();
        Ok(())
    }

    fn clear(&self) -> io::Result<()> {
        match fs::remove_file(&self.path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            other => other?,
        }
        self.revision.bump();
        Ok(())
    }

    fn changes(&self) -> watch::Receiver<u64> {
        self.revision.watch()
    }
}

fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr().cast_mut() }
}

/// Copies DPAPI's LocalAlloc'd output into a Vec and frees it.
///
/// # Safety
/// `out` must have been filled by a successful CryptProtectData/CryptUnprotectData call.
unsafe fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    // SAFETY: DPAPI guarantees pbData points at cbData readable bytes until LocalFree.
    let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    // SAFETY: pbData was allocated by DPAPI with LocalAlloc and is freed exactly once.
    unsafe { LocalFree(Some(HLOCAL(out.pbData.cast()))) };
    bytes
}

fn protect(plain: &[u8]) -> io::Result<Vec<u8>> {
    let input = blob(plain);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB::default();

    // SAFETY: every blob points at live slices for the duration of the call.
    unsafe {
        CryptProtectData(
            &input,
            PCWSTR::null(),
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
        .map_err(io::Error::other)?;
        Ok(take(out))
    }
}

fn unprotect(cipher: &[u8]) -> io::Result<Vec<u8>> {
    let input = blob(cipher);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB::default();

    // SAFETY: every blob points at live slices for the duration of the call.
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
        .map_err(io::Error::other)?;
        Ok(take(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_token_through_dpapi() {
        let dir = tempfile::tempdir().unwrap();
        let store = DpapiTokenStore::new(dir.path(), "install.token").unwrap();
        let token = InstallToken::new("tok_live", 76561198000000001, "Hexay");
        let mut changes = store.changes();

        assert_eq!(store.load(), None);
        store.save(&token).unwrap();
        assert_ne!(fs::read(dir.path().join("install.token")).unwrap(), serde_json::to_vec(&token).unwrap());
        assert_eq!(store.load(), Some(token));
        assert!(changes.has_changed().unwrap());
        changes.mark_unchanged();

        store.clear().unwrap();
        assert_eq!(store.load(), None);
        assert!(changes.has_changed().unwrap());
        store.clear().unwrap();
        assert!(!dir.path().join("install.token.tmp").exists());
    }

    #[test]
    fn an_undecryptable_file_reads_as_unenrolled() {
        let dir = tempfile::tempdir().unwrap();
        let store = DpapiTokenStore::new(dir.path(), "install.token").unwrap();
        fs::write(dir.path().join("install.token"), b"not dpapi").unwrap();

        assert_eq!(store.load(), None);
    }

    #[test]
    fn a_token_file_without_a_token_reads_as_unenrolled() {
        let dir = tempfile::tempdir().unwrap();
        let store = DpapiTokenStore::new(dir.path(), "install.token").unwrap();
        fs::write(dir.path().join("install.token"), protect(b"{}").unwrap()).unwrap();

        assert_eq!(store.load(), None);
    }
}
