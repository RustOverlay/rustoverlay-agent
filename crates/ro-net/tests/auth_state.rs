mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{MemoryTokenStore, hexay};
use ro_net::enrollment::{AuthState, InstallToken, TokenStore};

fn build() -> (Arc<AuthState>, Arc<MemoryTokenStore>) {
    let store = Arc::new(MemoryTokenStore::default());
    store.save(&hexay()).unwrap();
    (Arc::new(AuthState::new(store.clone())), store)
}

fn count_announcements(auth: &AuthState) -> Arc<AtomicUsize> {
    let announced = Arc::new(AtomicUsize::new(0));
    let counter = announced.clone();
    auth.subscribe(move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    announced
}

#[test]
fn an_install_with_a_token_starts_signed_in() {
    let (auth, _) = build();

    assert!(!auth.is_signed_out());
}

#[test]
fn signing_out_discards_the_rejected_token() {
    let (auth, store) = build();

    auth.sign_out("tok_live");

    assert!(auth.is_signed_out());
    assert_eq!(store.stored(), None);
}

/// Clearing the token is what silences the uplink; see RelayUplink::session.
#[test]
fn a_signed_out_install_has_nothing_left_to_broadcast_with() {
    let (auth, store) = build();

    auth.sign_out("tok_live");

    assert_eq!(store.load(), None);
}

#[test]
fn concurrent_rejections_announce_one_sign_out() {
    let (auth, _) = build();
    let announced = count_announcements(&auth);

    // Several requests can be in flight when a token is revoked, and they all come back 401.
    std::thread::scope(|scope| {
        for _ in 0..64 {
            scope.spawn(|| auth.sign_out("tok_live"));
        }
    });

    assert_eq!(announced.load(Ordering::SeqCst), 1);
}

/// A 401 for the old bearer, landing after the user already signed in again.
#[test]
fn a_late_rejection_of_a_replaced_token_keeps_the_new_one() {
    let (auth, store) = build();
    auth.sign_out("tok_live");
    store.save(&InstallToken::new("tok_fresh", 76561198000000001, "Hexay")).unwrap();
    auth.sign_in();

    auth.sign_out("tok_live");

    assert!(!auth.is_signed_out());
    assert_eq!(store.stored().map(|t| t.token).as_deref(), Some("tok_fresh"));
}

#[test]
fn signing_back_in_clears_the_banner() {
    let (auth, _) = build();
    auth.sign_out("tok_live");

    auth.sign_in();

    assert!(!auth.is_signed_out());
}

#[test]
fn signing_in_while_already_signed_in_announces_nothing() {
    let (auth, _) = build();
    let announced = count_announcements(&auth);

    auth.sign_in();

    assert_eq!(announced.load(Ordering::SeqCst), 0);
}
