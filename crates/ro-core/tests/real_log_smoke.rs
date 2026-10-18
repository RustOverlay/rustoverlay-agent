//! Runs the pipeline over whatever Rust install is on this machine. Skipped silently on any machine
//! without Rust — it exists to catch format drift after a game update, which fixtures cannot do.
//! Run with `--nocapture` to see the counts.

use ro_core::LogPipeline;
use ro_core::config::ClientConfig;
use ro_core::events::GameEvent;
use ro_core::install::{RustInstallLocator, SteamRustInstallLocator};
use ro_core::logs::read_all_lines;

#[test]
fn parses_the_local_install_log() {
    let Some(install) = SteamRustInstallLocator::default().locate().filter(|i| i.has_log()) else {
        println!("No local Rust install with a log — skipped.");
        return;
    };

    let lines = read_all_lines(&install.log_path()).unwrap();
    let events = LogPipeline::default().run_all(&lines);
    let config = ClientConfig::load(&install.client_config_path()).unwrap_or_default();

    let count = |pred: fn(&GameEvent) -> bool| events.iter().filter(|e| pred(e)).count();

    println!("install       {}", install.root().display());
    println!("lines         {}", lines.len());
    println!("events        {}", events.len());
    println!("combat        {}", count(|e| matches!(e, GameEvent::Combat(_))));
    println!("  kills       {}", count(|e| matches!(e, GameEvent::Combat(c) if c.is_kill())));
    println!("  ambiguous   {}", count(|e| matches!(e, GameEvent::Combat(c) if c.names_ambiguous)));
    println!("identity      {}", count(|e| matches!(e, GameEvent::Identity(_))));
    println!("player state  {}", count(|e| matches!(e, GameEvent::PlayerState(_))));
    println!("triples       {}", count(|e| matches!(e, GameEvent::Triple(_))));
    println!("transforms    {}", count(|e| matches!(e, GameEvent::Transform(_))));
    println!("screenmode    {:?}", config.screen_mode());
    println!("fov           {}", config.field_of_view());
    println!("nicknames     {}", config.uses_steam_nicknames());

    // Not an event count — a freshly truncated log legitimately has nothing in it yet.
    assert!(!lines.is_empty());
}
