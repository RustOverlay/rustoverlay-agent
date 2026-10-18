//! Grading belt readings against the fixed hotbar, and the score sheet printed at the end.

use crate::dotnet_format::fixed;

#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    Correct,
    Abstained,
    Wrong { says: String, expected: String },
}

impl Verdict {
    /// The per-frame dump's mark: `.` correct, `-` abstain, `X` wrong.
    pub fn mark(&self) -> char {
        match self {
            Verdict::Correct => '.',
            Verdict::Abstained => '-',
            Verdict::Wrong { .. } => 'X',
        }
    }
}

/// `named` is the answer and its look-alikes, answer first; None when the slot abstained.
pub fn judge(truth: &str, named: Option<&[String]>) -> Verdict {
    let should_be_empty = truth == "-" || truth == "?";

    let Some(named) = named else {
        // An empty slot abstaining is the right answer, not a miss.
        return if should_be_empty { Verdict::Correct } else { Verdict::Abstained };
    };

    // A look-alike set containing the truth is honest, not wrong: it reports that the icon alone
    // cannot separate these, which is exactly what the UI shows.
    if !should_be_empty && named.iter().any(|n| n == truth) {
        return Verdict::Correct;
    }

    Verdict::Wrong {
        says: named.join("|"),
        expected: if should_be_empty { "EMPTY".to_owned() } else { truth.to_owned() },
    }
}

#[derive(Debug)]
pub struct Tally {
    pub correct: u32,
    pub abstained: u32,
    pub wrong: u32,
    pub abstentions: Vec<u32>,
    pub mistakes: Vec<u32>,
    pub per_frame: Vec<String>,
}

impl Tally {
    pub fn new(slots: usize) -> Self {
        Self { correct: 0, abstained: 0, wrong: 0, abstentions: vec![0; slots], mistakes: vec![0; slots], per_frame: Vec::new() }
    }

    pub fn record(&mut self, slot: usize, verdict: &Verdict) {
        match verdict {
            Verdict::Correct => self.correct += 1,
            Verdict::Abstained => {
                self.abstained += 1;
                self.abstentions[slot] += 1;
            }
            Verdict::Wrong { .. } => {
                self.wrong += 1;
                self.mistakes[slot] += 1;
            }
        }
    }

    /// One dump line; marks for slots past the belt stay NUL, as the C# `char[]` did.
    pub fn record_frame(&mut self, date: &str, marks: &[char]) {
        self.per_frame.push(format!("  {date}  {}", marks.iter().collect::<String>()));
    }

    pub fn total(&self) -> u32 {
        self.correct + self.abstained + self.wrong
    }

    /// The closing score sheet, one entry per printed line.
    pub fn report(&self, truth: &[String]) -> Vec<String> {
        let total = self.total();
        let share = |n: u32| fixed(f64::from(n) * 100.0 / f64::from(total), 1);
        let mut lines = vec![
            String::new(),
            format!("slots scored : {total} over {} frames", self.per_frame.len()),
            format!("correct      : {} ({}%)", self.correct, share(self.correct)),
            format!("abstained    : {} ({}%)", self.abstained, share(self.abstained)),
            format!("WRONG        : {} ({}%)", self.wrong, share(self.wrong)),
            String::new(),
        ];

        for (slot, truth) in truth.iter().enumerate() {
            lines.push(format!(
                "  slot {slot} {truth:<18} abstained {:>4}   wrong {:>4}",
                self.abstentions[slot], self.mistakes[slot]
            ));
        }

        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(n: &[&str]) -> Vec<String> {
        n.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn abstaining_on_an_empty_slot_is_correct() {
        assert_eq!(judge("-", None), Verdict::Correct);
        assert_eq!(judge("?", None), Verdict::Correct);
        assert_eq!(judge("rock", None), Verdict::Abstained);
    }

    #[test]
    fn a_look_alike_set_holding_the_truth_is_correct() {
        assert_eq!(judge("ammo.rifle", Some(&names(&["ammo.snowballgun", "ammo.rifle"]))), Verdict::Correct);
    }

    #[test]
    fn any_answer_on_an_empty_slot_is_wrong() {
        let verdict = judge("-", Some(&names(&["water", "rock"])));

        assert_eq!(verdict, Verdict::Wrong { says: "water|rock".to_owned(), expected: "EMPTY".to_owned() });
        assert_eq!(verdict.mark(), 'X');
    }

    #[test]
    fn the_report_matches_the_csharp_layout() {
        let truth = names(&["rock", "-"]);
        let mut tally = Tally::new(2);
        tally.record(0, &Verdict::Correct);
        tally.record(0, &Verdict::Abstained);
        tally.record(1, &Verdict::Wrong { says: "x".into(), expected: "EMPTY".into() });
        tally.record_frame("20260809", &['.', 'X']);

        assert_eq!(
            tally.report(&truth),
            vec![
                "",
                "slots scored : 3 over 1 frames",
                "correct      : 1 (33.3%)",
                "abstained    : 1 (33.3%)",
                "WRONG        : 1 (33.3%)",
                "",
                "  slot 0 rock               abstained    1   wrong    0",
                "  slot 1 -                  abstained    0   wrong    1",
            ]
        );
        assert_eq!(tally.per_frame, vec!["  20260809  .X"]);
    }
}
