//! Use cases and the ports the outside world plugs into. To support something new:
//! a new pet format implements [`PetSource`], a new place to show the pet (an OS, a
//! terminal, an editor panel) implements [`Stage`].
use familiard_domain::{Companion, Mood, Pet, Sprite};
use std::error::Error;
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

/// Driver port for stages: runs the companion, without knowing about any display.
pub struct Player {
    companion: Companion,
}

impl Player {
    pub fn new(pet: Pet) -> Self {
        Self { companion: Companion::new(pet) }
    }

    /// The box a stage needs so that no frame ever has to resize it.
    pub fn bounds(&self) -> (u32, u32) {
        self.companion.pet().bounds()
    }

    pub fn sprite(&self) -> &Sprite {
        self.companion.sprite()
    }

    /// Returns the sprite to draw when the visible frame changed.
    pub fn tick(&mut self, elapsed: Duration) -> Option<&Sprite> {
        self.companion.advance(elapsed).then(|| self.companion.sprite())
    }

    pub fn next_frame_in(&self) -> Duration {
        self.companion.next_frame_in()
    }

    pub fn feel(&mut self, mood: Mood) {
        self.companion.feel(mood);
    }
}

/// The main use case: bring a pet to a stage.
pub fn summon(source: &dyn PetSource, stage: Box<dyn Stage>) -> Result<(), Failure> {
    stage.perform(Player::new(source.load()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use familiard_domain::Clip;
    use std::collections::HashMap;
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
        summon(&FixedPet, Box::new(RecordingStage(shown.clone()))).unwrap();
        assert_eq!(*shown.lock().unwrap(), vec![2, 1, 2]);
    }

    #[test]
    fn a_broken_source_never_reaches_the_stage() {
        let shown = Arc::new(Mutex::new(Vec::new()));
        let error = summon(&BrokenPet, Box::new(RecordingStage(shown.clone()))).unwrap_err();
        assert_eq!(error.to_string(), "no such pet");
        assert!(shown.lock().unwrap().is_empty());
    }
}
