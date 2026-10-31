use crate::Mood;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

const CELEBRATION: Duration = Duration::from_secs(3);
const DONE_LINGER: Duration = Duration::from_secs(30);
const STALE_REGISTERED: Duration = Duration::from_secs(90);
const STALE_ACTIVE: Duration = Duration::from_secs(300);
/// Far more sessions than a person runs; keeps a noisy local client from growing this.
const MAX_SESSIONS: usize = 256;

/// What an agent session says it is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Registered,
    Working,
    Waiting,
    Done,
    Ended,
}

/// One session reporting its state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub session: String,
    pub signal: Signal,
    /// Where the session runs, when the agent says so.
    pub folder: Option<String>,
}

#[derive(Debug)]
struct Session {
    signal: Signal,
    quiet: Duration,
}

/// Which sessions a pet listens to.
#[derive(Debug)]
enum Interest {
    Everything,
    /// Waiting for the next session to report from this folder or below it.
    Folder(String),
    Session(String),
}

/// Turns what every agent session reports into one mood. Work outranks waiting, and
/// waiting outranks being done. Sessions that go quiet are forgotten, because an agent
/// can die without saying goodbye.
#[derive(Debug)]
pub struct Activity {
    sessions: HashMap<String, Session>,
    settled: Mood,
    celebrating: Duration,
    interest: Interest,
    finished: bool,
}

impl Default for Activity {
    fn default() -> Self {
        Self {
            sessions: HashMap::new(),
            settled: Mood::Idle,
            celebrating: Duration::ZERO,
            interest: Interest::Everything,
            finished: false,
        }
    }
}

impl Activity {
    /// Hears only the next session to report from `folder` or below it, and then only
    /// that session. A session ending is not a claim: it was never this pet's to follow.
    pub fn following(folder: impl Into<String>) -> Self {
        Self { interest: Interest::Folder(folder.into()), ..Self::default() }
    }

    fn cares_about(&mut self, report: &Report) -> bool {
        match &self.interest {
            Interest::Everything => true,
            Interest::Session(session) => *session == report.session,
            Interest::Folder(folder) => {
                let inside = |reported: &String| Path::new(reported).starts_with(folder);
                let claims = report.signal != Signal::Ended && report.folder.as_ref().is_some_and(inside);
                if claims {
                    self.interest = Interest::Session(report.session.clone());
                }
                claims
            }
        }
    }

    pub fn record(&mut self, report: Report) {
        if !self.cares_about(&report) {
            return;
        }
        if report.signal == Signal::Ended {
            self.sessions.remove(&report.session);
            self.finished = matches!(self.interest, Interest::Session(_));
        } else if self.sessions.len() < MAX_SESSIONS || self.sessions.contains_key(&report.session) {
            self.sessions.insert(report.session, Session { signal: report.signal, quiet: Duration::ZERO });
        }
        self.settle();
    }

    pub fn advance(&mut self, elapsed: Duration) {
        for session in self.sessions.values_mut() {
            session.quiet += elapsed;
        }
        self.sessions.retain(|_, session| {
            session.quiet
                <= match session.signal {
                    Signal::Done => DONE_LINGER,
                    Signal::Registered => STALE_REGISTERED,
                    _ => STALE_ACTIVE,
                }
        });
        self.celebrating = self.celebrating.saturating_sub(elapsed);
        self.settle();
    }

    /// Whether the session this pet was following has ended. A session that merely goes
    /// quiet has not: a finished agent often sits idle for a long time.
    pub fn finished(&self) -> bool {
        self.finished
    }

    pub fn mood(&self) -> Mood {
        if self.celebrating > Duration::ZERO {
            Mood::Celebrating
        } else {
            self.settled
        }
    }

    fn settle(&mut self) {
        let any = |signal| self.sessions.values().any(|session| session.signal == signal);
        let mood = if any(Signal::Working) {
            Mood::Working
        } else if any(Signal::Waiting) {
            Mood::Waiting
        } else if any(Signal::Done) {
            Mood::Done
        } else {
            Mood::Idle
        };
        if mood != Mood::Done {
            self.celebrating = Duration::ZERO;
        } else if self.settled != Mood::Done {
            self.celebrating = CELEBRATION;
        }
        self.settled = mood;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(session: &str, signal: Signal) -> Report {
        Report { session: session.into(), signal, folder: None }
    }

    fn from(session: &str, folder: &str, signal: Signal) -> Report {
        Report { session: session.into(), signal, folder: Some(folder.into()) }
    }

    fn activity(reports: &[(&str, Signal)]) -> Activity {
        let mut activity = Activity::default();
        for (session, signal) in reports {
            activity.record(report(session, *signal));
        }
        activity
    }

    #[test]
    fn starts_idle_and_an_open_agent_is_not_work() {
        assert_eq!(Activity::default().mood(), Mood::Idle);
        assert_eq!(activity(&[("a", Signal::Registered)]).mood(), Mood::Idle);
    }

    #[test]
    fn working_outranks_waiting_and_waiting_outranks_done() {
        assert_eq!(activity(&[("a", Signal::Waiting), ("b", Signal::Working)]).mood(), Mood::Working);
        let mut pet = activity(&[("a", Signal::Done)]);
        pet.advance(CELEBRATION);
        pet.record(report("b", Signal::Waiting));
        assert_eq!(pet.mood(), Mood::Waiting);
    }

    #[test]
    fn finishing_celebrates_then_settles_then_returns_to_idle() {
        let mut pet = activity(&[("a", Signal::Working), ("a", Signal::Done)]);
        assert_eq!(pet.mood(), Mood::Celebrating);
        pet.advance(CELEBRATION);
        assert_eq!(pet.mood(), Mood::Done);
        pet.advance(DONE_LINGER + Duration::from_secs(1));
        assert_eq!(pet.mood(), Mood::Idle);
    }

    #[test]
    fn nothing_is_celebrated_while_another_session_still_works() {
        let pet = activity(&[("a", Signal::Working), ("b", Signal::Working), ("a", Signal::Done)]);
        assert_eq!(pet.mood(), Mood::Working);
    }

    #[test]
    fn new_work_interrupts_a_celebration_and_finishing_again_celebrates_again() {
        let mut pet = activity(&[("a", Signal::Done)]);
        pet.record(report("a", Signal::Working));
        assert_eq!(pet.mood(), Mood::Working);
        pet.record(report("a", Signal::Done));
        assert_eq!(pet.mood(), Mood::Celebrating);
    }

    #[test]
    fn an_ended_session_is_forgotten_at_once() {
        let pet = activity(&[("a", Signal::Working), ("a", Signal::Ended)]);
        assert_eq!(pet.mood(), Mood::Idle);
    }

    #[test]
    fn a_session_that_goes_quiet_is_forgotten() {
        let mut pet = activity(&[("a", Signal::Working)]);
        pet.advance(STALE_ACTIVE);
        assert_eq!(pet.mood(), Mood::Working);
        pet.advance(Duration::from_secs(1));
        assert_eq!(pet.mood(), Mood::Idle);
    }

    #[test]
    fn news_from_a_session_keeps_it_alive() {
        let mut pet = activity(&[("a", Signal::Working)]);
        pet.advance(STALE_ACTIVE);
        pet.record(report("a", Signal::Working));
        pet.advance(STALE_ACTIVE);
        assert_eq!(pet.mood(), Mood::Working);
    }

    #[test]
    fn the_number_of_sessions_is_bounded_but_known_ones_still_update() {
        let mut pet = Activity::default();
        for i in 0..MAX_SESSIONS + 10 {
            pet.record(report(&format!("s{i}"), Signal::Registered));
        }
        assert_eq!(pet.sessions.len(), MAX_SESSIONS);
        pet.record(report("s0", Signal::Working));
        assert_eq!(pet.mood(), Mood::Working);
    }

    #[test]
    fn a_bound_pet_stays_idle_and_ignores_other_folders_until_a_session_reports_from_its_own() {
        let mut pet = Activity::following("/work/app");
        assert_eq!(pet.mood(), Mood::Idle);
        pet.record(from("other", "/work/site", Signal::Working));
        pet.record(report("no-folder", Signal::Working));
        assert_eq!(pet.mood(), Mood::Idle);
        pet.record(from("mine", "/work/app", Signal::Working));
        assert_eq!(pet.mood(), Mood::Working);
    }

    #[test]
    fn the_first_report_from_the_folder_claims_the_pet_for_that_session_only() {
        let mut pet = Activity::following("/work/app");
        pet.record(from("mine", "/work/app", Signal::Waiting));
        pet.record(from("second", "/work/app", Signal::Working));
        pet.record(from("second", "/work/app", Signal::Ended));
        assert_eq!(pet.mood(), Mood::Waiting);
        assert!(!pet.finished());
        // Once claimed the session counts wherever it goes, and with no folder at all.
        pet.record(from("mine", "/elsewhere", Signal::Working));
        assert_eq!(pet.mood(), Mood::Working);
        pet.record(report("mine", Signal::Done));
        assert_eq!(pet.mood(), Mood::Celebrating);
    }

    #[test]
    fn a_subfolder_matches_but_a_folder_with_the_same_prefix_does_not() {
        let mut pet = Activity::following("/a/b");
        pet.record(from("sibling", "/a/bc", Signal::Working));
        assert_eq!(pet.mood(), Mood::Idle);
        pet.record(from("parent", "/a", Signal::Working));
        assert_eq!(pet.mood(), Mood::Idle);
        pet.record(from("inside", "/a/b/deep/down", Signal::Working));
        assert_eq!(pet.mood(), Mood::Working);
    }

    #[test]
    fn the_end_of_a_session_nobody_claimed_is_not_a_claim() {
        let mut pet = Activity::following("/work/app");
        pet.record(from("older", "/work/app", Signal::Ended));
        assert!(!pet.finished());
        pet.record(from("next", "/work/app", Signal::Working));
        assert_eq!(pet.mood(), Mood::Working);
    }

    #[test]
    fn a_bound_pet_finishes_when_its_session_ends_and_only_then() {
        let mut pet = Activity::following("/work/app");
        pet.record(from("mine", "/work/app", Signal::Working));
        assert!(!pet.finished());
        pet.record(report("mine", Signal::Done));
        assert!(!pet.finished());
        pet.record(report("mine", Signal::Ended));
        assert!(pet.finished());
        assert_eq!(pet.mood(), Mood::Idle);
    }

    #[test]
    fn a_session_that_goes_quiet_keeps_its_pet_and_can_come_back() {
        let mut pet = Activity::following("/work/app");
        pet.record(from("mine", "/work/app", Signal::Working));
        pet.advance(STALE_ACTIVE + STALE_ACTIVE);
        assert_eq!(pet.mood(), Mood::Idle);
        assert!(!pet.finished());
        pet.record(from("other", "/work/app", Signal::Working));
        assert_eq!(pet.mood(), Mood::Idle);
        pet.record(report("mine", Signal::Working));
        assert_eq!(pet.mood(), Mood::Working);
    }

    #[test]
    fn a_global_pet_hears_everyone_and_never_finishes() {
        let mut pet = activity(&[("a", Signal::Working)]);
        pet.record(from("b", "/anywhere", Signal::Waiting));
        pet.record(report("a", Signal::Ended));
        assert_eq!(pet.mood(), Mood::Waiting);
        pet.record(report("b", Signal::Ended));
        assert!(!pet.finished());
    }
}
