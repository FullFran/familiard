//! Use cases and the ports the outside world plugs into. To support something new:
//! a new pet format implements [`PetSource`], a new place to show the pet (an OS, a
//! terminal, an editor panel) implements [`Stage`], and a new way of hearing about
//! activity implements [`ActivityFeed`].
use familiard_domain::{Activity, Companion, Pet, Report, Sprite};
use std::error::Error;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

pub type Failure = Box<dyn Error + Send + Sync>;

/// Driven port: where a pet comes from.
pub trait PetSource {
    fn load(&self) -> Result<Pet, Failure>;
}

/// Driven port: somewhere a pet can be shown. A stage owns its event loop and asks
/// the [`Player`] what to draw, so it fits toolkits that insist on running the loop.
pub trait Stage {
    fn perform(self: Box<Self>, player: Player) -> Result<(), Failure>;
}

/// Driver port: something that tells the pet what the agents are doing. It delivers
/// reports from its own thread for as long as the inbox stays open.
pub trait ActivityFeed {
    fn listen(self: Box<Self>, inbox: Sender<Report>) -> Result<(), Failure>;
}

/// Driver port for stages: runs the companion, without knowing about any display.
pub struct Player {
    companion: Companion,
    activity: Activity,
    inbox: Receiver<Report>,
}

impl Player {
    pub fn new(pet: Pet, inbox: Receiver<Report>) -> Self {
        Self { companion: Companion::new(pet), activity: Activity::default(), inbox }
    }

    /// The box a stage needs so that no frame ever has to resize it.
    pub fn bounds(&self) -> (u32, u32) {
        self.companion.pet().bounds()
    }

    pub fn sprite(&self) -> &Sprite {
        self.companion.sprite()
    }

    /// Reads what happened since the last tick. Returns the sprite to draw when what
    /// should be on screen changed.
    pub fn tick(&mut self, elapsed: Duration) -> Option<&Sprite> {
        while let Ok(report) = self.inbox.try_recv() {
            self.activity.record(report);
        }
        self.activity.advance(elapsed);
        let mood = self.activity.mood();
        let new_mood = mood != self.companion.mood();
        self.companion.feel(mood);
        let new_frame = self.companion.advance(if new_mood { Duration::ZERO } else { elapsed });
        (new_mood || new_frame).then(|| self.companion.sprite())
    }

    pub fn next_frame_in(&self) -> Duration {
        self.companion.next_frame_in()
    }
}

/// The main use case: bring a pet to a stage.
pub fn summon(source: &dyn PetSource, stage: Box<dyn Stage>, inbox: Receiver<Report>) -> Result<(), Failure> {
    stage.perform(Player::new(source.load()?, inbox))
}

#[cfg(test)]
mod tests {
    use super::*;
    use familiard_domain::{Clip, Mood, Signal};
    use std::collections::HashMap;
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};

    struct FixedPet;
    impl PetSource for FixedPet {
        fn load(&self) -> Result<Pet, Failure> {
            let frames = (1..=2).map(|id| Sprite::new(1, 1, vec![id, 0, 0, 255]).unwrap()).collect();
            let clips = HashMap::from([(Mood::Idle, Clip::new(frames).unwrap())]);
            Ok(Pet::new("fixed", clips)?)
        }
    }

    struct BrokenPet;
    impl PetSource for BrokenPet {
        fn load(&self) -> Result<Pet, Failure> {
            Err("no such pet".into())
        }
    }

    struct RecordingStage(Arc<Mutex<Vec<u8>>>);
    impl Stage for RecordingStage {
        fn perform(self: Box<Self>, mut player: Player) -> Result<(), Failure> {
            for _ in 0..3 {
                let wait = player.next_frame_in();
                if let Some(sprite) = player.tick(wait) {
                    self.0.lock().unwrap().push(sprite.rgba()[0]);
                }
            }
            Ok(())
        }
    }

    #[test]
    fn a_stage_plays_the_pet_from_any_source() {
        let shown = Arc::new(Mutex::new(Vec::new()));
        summon(&FixedPet, Box::new(RecordingStage(shown.clone())), channel().1).unwrap();
        assert_eq!(*shown.lock().unwrap(), vec![2, 1, 2]);
    }

    #[test]
    fn a_report_changes_the_picture_on_the_next_tick() {
        let working = Sprite::new(1, 1, vec![9, 0, 0, 255]).unwrap();
        let idle = Sprite::new(1, 1, vec![1, 0, 0, 255]).unwrap();
        let clips = HashMap::from([
            (Mood::Idle, Clip::new(vec![idle]).unwrap()),
            (Mood::Working, Clip::new(vec![working]).unwrap()),
        ]);
        let (outbox, inbox) = channel();
        let mut player = Player::new(Pet::new("p", clips).unwrap(), inbox);
        assert!(player.tick(Duration::ZERO).is_none());
        outbox.send(Report { session: "a".into(), signal: Signal::Working }).unwrap();
        assert_eq!(player.tick(Duration::ZERO).unwrap().rgba()[0], 9);
        assert!(player.tick(Duration::ZERO).is_none());
    }

    #[test]
    fn a_closed_inbox_is_not_a_problem() {
        let (outbox, inbox) = channel::<Report>();
        drop(outbox);
        let mut player = FixedPet.load().map(|pet| Player::new(pet, inbox)).unwrap();
        assert!(player.tick(Duration::ZERO).is_none());
    }

    #[test]
    fn a_broken_source_never_reaches_the_stage() {
        let shown = Arc::new(Mutex::new(Vec::new()));
        let error = summon(&BrokenPet, Box::new(RecordingStage(shown.clone())), channel().1).unwrap_err();
        assert_eq!(error.to_string(), "no such pet");
        assert!(shown.lock().unwrap().is_empty());
    }
}
