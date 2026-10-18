//! Keeps the agent's heavy work behind the game in the scheduler's queue.
//!
//! Slot matching, skin decoding and catalogue builds run on rayon's pool and the vision thread.
//! At normal priority a burst — joining a server sends hundreds of new skins at once — competes
//! with the game for every core, and the game is what the player is watching. Below normal, the
//! work takes only what the game leaves; on an idle core it runs exactly as fast.

use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL};

/// Lowers the calling thread. A failure leaves it at normal priority, which only costs smoothness.
pub fn lower_current_thread() {
    // SAFETY: the pseudo-handle names the calling thread and needs no closing.
    if let Err(error) = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL) } {
        tracing::debug!(%error, "Could not lower a worker thread's priority");
    }
}

/// Builds rayon's global pool with every worker lowered. Must run before anything uses rayon.
pub fn lower_rayon_pool() {
    let built = rayon::ThreadPoolBuilder::new()
        .thread_name(|i| format!("vision-worker-{i}"))
        .start_handler(|_| lower_current_thread())
        .build_global();
    if let Err(error) = built {
        tracing::warn!(%error, "rayon's pool already exists; its workers stay at normal priority");
    }
}
