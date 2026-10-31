use crate::Mood;
use std::collections::HashMap;
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
}

#[derive(Debug)]
struct Session {
    signal: Signal,
    quiet: Duration,
}

/// Turns what every agent session reports into one mood. Work outranks waiting, and
/// waiting outranks being done. Sessions that go quiet are forgotten, because an agent
/// can die without saying goodbye.
#[derive(Debug)]
pub struct Activity {
    sessions: HashMap<String, Session>,
    settled: Mood,
    celebrating: Duration,
}

impl Default for Activity {
    fn default() -> Self {
        Self { sessions: HashMap::new(), settled: Mood::Idle, celebrating: Duration::ZERO }
    }
}

impl Activity {
    pub fn record(&mut self, report: Report) {
        if report.signal == Signal::Ended {
            self.sessions.remove(&report.session);
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
        Report { session: session.into(), signal }
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
}
