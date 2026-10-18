use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// RFC 7636 S256 pair binding a sign-in's one-time code to this `enroll()` call.
///
/// Without it, any local page that finds the loopback port can hand the agent a code minted for
/// its own account. The challenge goes out on `/auth/steam/start`; the verifier never leaves this
/// process until `/auth/exchange`, over TLS.
pub(crate) struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    pub fn generate() -> std::io::Result<Self> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
        let verifier = URL_SAFE_NO_PAD.encode(bytes);
        let challenge = challenge_of(&verifier);
        Ok(Self { verifier, challenge })
    }
}

fn challenge_of(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_rfc_7636_appendix_b_vector() {
        assert_eq!(
            challenge_of("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn generates_43_char_pairs_that_differ_per_call() {
        let a = Pkce::generate().unwrap();
        let b = Pkce::generate().unwrap();
        assert_eq!(a.verifier.len(), 43);
        assert_eq!(a.challenge.len(), 43);
        assert_eq!(a.challenge, challenge_of(&a.verifier));
        assert_ne!(a.verifier, b.verifier);
    }
}
