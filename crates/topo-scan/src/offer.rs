//! What a proposal offers now: from its state and what trying it on the
//! files as they are gives, what a view of it says, what its two sides are
//! called, and what can be done with it. One table, so that a view, the
//! thread and an agent all say the same, and none of them decides it.
//!
//! A view of a proposal shows what its button would do: applying an open one
//! (the files now, then with it), or reverting an applied one (the files now,
//! then without it). Where that cannot be done, it is shown as it was
//! proposed (the files then, before and after), and says why.

use serde::Serialize;

/// What trying a change on the files as they are gave.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// It applies. `moved`: the scripts have changed since it was made. `empty`: the files are already as it leaves them.
    Clean { moved: bool, empty: bool },
    /// It does not: these files changed where it edits.
    Conflict(Vec<String>),
    /// It applies, but the model does not then evaluate.
    Broken(String),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Offer {
    /// "open", "applied", "rejected" or "reverted".
    pub state: String,
    /// A few words for the view's bar, after the proposal's name ("" if there is nothing to say).
    pub label: String,
    /// More, where something is in the way.
    pub note: String,
    /// "apply" or "revert", if either belongs to this state.
    pub action: Option<&'static str>,
    /// Whether that can be done now.
    pub enabled: bool,
    /// What the two sides shown are called.
    pub before: &'static str,
    pub after: &'static str,
    /// Shown as it was proposed (the files then), not as it would change the files now.
    pub as_proposed: bool,
    /// Whether there is anything to show (not, if as it was proposed it no longer evaluates).
    pub shown: bool,
}

impl Offer {
    /// As it was proposed it no longer evaluates (the script's API has moved on since): nothing is shown, and why.
    pub fn unshown(mut self, why: &str) -> Offer {
        self.shown = false;
        self.note = format!("{}{}As it was proposed it no longer evaluates, so it cannot be shown: {why}", self.note, if self.note.is_empty() { "" } else { " " });
        self
    }
}

/// `applied`: who applied it and how long ago, for an applied one.
pub fn offer(state: &str, outcome: Option<&Outcome>, applied: Option<(&str, &str)>) -> Offer {
    let then = |label: String, note: String, action: Option<&'static str>| Offer { state: state.into(), label, note, action, enabled: false, before: "Before (then)", after: "After (then)", as_proposed: true, shown: true };
    let by = applied.map_or("applied".to_string(), |(who, when)| format!("applied by {who}, {when}"));
    let files = |f: &[String]| if f.is_empty() { "the files".to_string() } else { f.join(", ") };
    match (state, outcome) {
        ("open", Some(Outcome::Clean { empty: true, .. })) => then("already in the files".into(), "The files are already as it leaves them: there is nothing to apply. Shown as it was proposed.".into(), Some("apply")),
        ("open", Some(Outcome::Clean { moved, .. })) => Offer {
            state: state.into(),
            label: if *moved { "on the model as it is now (proposed on an older one)".into() } else { String::new() },
            note: String::new(),
            action: Some("apply"),
            enabled: true,
            before: "Before",
            after: "After",
            as_proposed: false,
            shown: true,
        },
        ("open", Some(Outcome::Conflict(f))) => then("out of date".into(), format!("It no longer applies: {} changed where it edits. Shown as it was proposed; it needs proposing again on the files as they are.", files(f)), Some("apply")),
        ("open", Some(Outcome::Broken(e))) => then("would break the model".into(), format!("It applies to the files, but the model does not then evaluate: {e}. Shown as it was proposed."), Some("apply")),
        ("applied", Some(Outcome::Clean { empty: true, .. })) => then(by, "The files are already as they were before it: there is nothing to revert. Shown as it was proposed.".into(), Some("revert")),
        ("applied", Some(Outcome::Clean { moved, .. })) => Offer {
            state: state.into(),
            label: by,
            note: if *moved { "Reverting undoes only this: what changed since stays.".into() } else { String::new() },
            action: Some("revert"),
            enabled: true,
            before: "Now",
            after: "If reverted",
            as_proposed: false,
            shown: true,
        },
        ("applied", Some(Outcome::Conflict(f))) => then(by, format!("It cannot be reverted as it is: later changes to {} build on it. Shown as it was proposed.", files(f)), Some("revert")),
        ("applied", Some(Outcome::Broken(e))) => then(by, format!("Reverting it would leave a model that does not evaluate: {e}. Shown as it was proposed."), Some("revert")),
        _ => then(state.to_string(), String::new(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_says_what_it_shows_and_what_can_be_done() {
        let clean = |moved, empty| Outcome::Clean { moved, empty };
        let who = Some(("Steven", "2 hours ago"));
        // (state, outcome) → (action, enabled, as proposed, the sides, a word of the label or note)
        let rows: Vec<(&str, Option<Outcome>, Option<&str>, bool, bool, (&str, &str), &str)> = vec![
            ("open", Some(clean(false, false)), Some("apply"), true, false, ("Before", "After"), ""),
            ("open", Some(clean(true, false)), Some("apply"), true, false, ("Before", "After"), "as it is now"),
            ("open", Some(clean(true, true)), Some("apply"), false, true, ("Before (then)", "After (then)"), "already"),
            ("open", Some(Outcome::Conflict(vec!["garage.ts".into()])), Some("apply"), false, true, ("Before (then)", "After (then)"), "garage.ts"),
            ("open", Some(Outcome::Broken("no node named X".into())), Some("apply"), false, true, ("Before (then)", "After (then)"), "no node named X"),
            ("applied", Some(clean(false, false)), Some("revert"), true, false, ("Now", "If reverted"), "applied by Steven, 2 hours ago"),
            ("applied", Some(clean(true, false)), Some("revert"), true, false, ("Now", "If reverted"), "what changed since stays"),
            ("applied", Some(clean(true, true)), Some("revert"), false, true, ("Before (then)", "After (then)"), "nothing to revert"),
            ("applied", Some(Outcome::Conflict(vec!["garage.ts".into()])), Some("revert"), false, true, ("Before (then)", "After (then)"), "build on it"),
            ("applied", Some(Outcome::Broken("e".into())), Some("revert"), false, true, ("Before (then)", "After (then)"), "does not evaluate"),
            ("rejected", None, None, false, true, ("Before (then)", "After (then)"), "rejected"),
            ("reverted", None, None, false, true, ("Before (then)", "After (then)"), "reverted"),
        ];
        for (state, outcome, action, enabled, as_proposed, sides, word) in rows {
            let o = offer(state, outcome.as_ref(), if state == "applied" { who } else { None });
            assert_eq!((o.action, o.enabled, o.as_proposed, (o.before, o.after)), (action, enabled, as_proposed, sides), "{state} {outcome:?}: {o:?}");
            assert!(format!("{} {}", o.label, o.note).contains(word), "{state} {outcome:?}: {o:?}");
            // What cannot be done says why; what is shown as proposed says so.
            assert!(o.enabled || o.action.is_none() || !o.note.is_empty(), "{o:?}");
            assert!(o.shown);
        }
        // One that no longer evaluates as it was proposed: not shown, and the reason is added to what was said.
        let o = offer("open", Some(&Outcome::Conflict(vec!["garage.ts".into()])), None).unshown("no `at`");
        assert!(!o.shown && o.note.contains("garage.ts") && o.note.contains("no `at`"), "{o:?}");
    }
}
