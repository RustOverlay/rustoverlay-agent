use crate::correlation::TransformCorrelator;
use crate::events::GameEvent;
use crate::hosting::AgentOptions;
use crate::logs::{LogBlock, LogBlockAssembler};
use crate::parsing::{CombatLogDeduplicator, combat_log_parser, player_parser, transform_parser};

/// Lines → blocks → parsers → domain events. Takes lines rather than opening a source, so the same
/// pipeline runs over a live tail, a fixture, or a replay.
///
/// Stateful — the block assembler, deduplicator and correlator all carry context between lines —
/// so one instance belongs to one run and must not be shared.
#[derive(Debug)]
pub struct LogPipeline {
    assembler: LogBlockAssembler,
    dedupe: CombatLogDeduplicator,
    correlator: TransformCorrelator,
}

impl Default for LogPipeline {
    fn default() -> Self {
        Self::new(&AgentOptions::default())
    }
}

impl LogPipeline {
    pub fn new(options: &AgentOptions) -> Self {
        Self {
            assembler: LogBlockAssembler::new(),
            dedupe: CombatLogDeduplicator::new(Some(options.dedupe_tolerance), Some(options.dedupe_retention)),
            correlator: TransformCorrelator::new(Some(options.correlation_window)),
        }
    }

    /// Feeds one line; returns the events of the block it completed, if any.
    pub fn push_line(&mut self, line: &str) -> Vec<GameEvent> {
        match self.assembler.add(line) {
            Some(block) => self.interpret(&block),
            None => Vec::new(),
        }
    }

    /// Releases the block in progress. Call when the line source ends.
    pub fn flush(&mut self) -> Vec<GameEvent> {
        match self.assembler.flush() {
            Some(block) => self.interpret(&block),
            None => Vec::new(),
        }
    }

    /// Runs the whole of an already-materialised log, flushing at the end. Used by tests and replay.
    pub fn run_all<S: AsRef<str>>(&mut self, lines: impl IntoIterator<Item = S>) -> Vec<GameEvent> {
        let mut events: Vec<GameEvent> = lines.into_iter().flat_map(|line| self.push_line(line.as_ref())).collect();
        events.extend(self.flush());
        events
    }

    pub fn interpret(&mut self, block: &LogBlock) -> Vec<GameEvent> {
        if combat_log_parser::is_header(&block.message) {
            return self.dedupe.filter(combat_log_parser::parse(block)).into_iter().map(GameEvent::Combat).collect();
        }

        self.interpret_single(block)
    }

    fn interpret_single(&mut self, block: &LogBlock) -> Vec<GameEvent> {
        if let Some(identity) = player_parser::try_parse_identity(block) {
            return vec![GameEvent::Identity(identity)];
        }

        if let Some(state) = player_parser::try_parse_player_state(block) {
            return vec![GameEvent::PlayerState(state)];
        }

        let mut events = Vec::new();
        for triple in transform_parser::parse(block) {
            let transform = self.correlator.accept(triple.clone());
            events.push(GameEvent::Triple(triple));
            events.extend(transform.map(GameEvent::Transform));
        }
        events
    }
}
