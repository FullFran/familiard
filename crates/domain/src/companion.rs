use crate::{Clip, Mood, Pet, Sprite};
use std::time::Duration;

/// A pet alive on some stage: which mood it is in and which frame it is showing.
#[derive(Debug, Clone)]
pub struct Companion {
    pet: Pet,
    mood: Mood,
    frame: usize,
    since_frame: Duration,
}

impl Companion {
    pub fn new(pet: Pet) -> Self {
        Self { pet, mood: Mood::Idle, frame: 0, since_frame: Duration::ZERO }
    }

    pub fn pet(&self) -> &Pet {
        &self.pet
    }

    pub fn mood(&self) -> Mood {
        self.mood
    }

    /// Changing mood restarts the animation; repeating the current one does not.
    pub fn feel(&mut self, mood: Mood) {
        if mood != self.mood {
            self.mood = mood;
            self.frame = 0;
            self.since_frame = Duration::ZERO;
        }
    }

    fn clip(&self) -> &Clip {
        self.pet.clip(self.mood)
    }

    pub fn sprite(&self) -> &Sprite {
        self.clip().frame(self.frame)
    }

    fn frame_time(&self) -> Duration {
        Duration::from_secs(1) / self.mood.fps()
    }

    /// Advances time. Returns whether the visible frame changed.
    pub fn advance(&mut self, elapsed: Duration) -> bool {
        self.since_frame += elapsed;
        let frame_time = self.frame_time();
        let mut changed = false;
        while self.since_frame >= frame_time {
            self.since_frame -= frame_time;
            self.frame = (self.frame + 1) % self.clip().len();
            changed = true;
        }
        changed
    }

    /// How long until the next frame is due.
    pub fn next_frame_in(&self) -> Duration {
        self.frame_time().saturating_sub(self.since_frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PetError;
    use std::collections::HashMap;

    fn sprite(id: u8) -> Sprite {
        Sprite::new(1, 1, vec![id, 0, 0, 255]).unwrap()
    }

    fn clip(ids: &[u8]) -> Clip {
        Clip::new(ids.iter().map(|id| sprite(*id)).collect()).unwrap()
    }

    fn companion() -> Companion {
        let clips = HashMap::from([(Mood::Idle, clip(&[1, 2])), (Mood::Working, clip(&[7, 8, 9]))]);
        Companion::new(Pet::new("test", clips).unwrap())
    }

    const THIRD: Duration = Duration::from_nanos(333_333_333);

    #[test]
    fn a_pet_cannot_exist_without_an_idle_clip() {
        let clips = HashMap::from([(Mood::Working, clip(&[1]))]);
        assert_eq!(Pet::new("x", clips).unwrap_err(), PetError::MissingIdleClip);
    }

    #[test]
    fn frames_advance_at_the_pace_of_the_mood_and_loop() {
        let mut pet = companion();
        assert!(!pet.advance(Duration::from_millis(100)));
        assert_eq!(pet.sprite(), &sprite(1));
        assert!(pet.advance(THIRD));
        assert_eq!(pet.sprite(), &sprite(2));
        assert!(pet.advance(THIRD));
        assert_eq!(pet.sprite(), &sprite(1));
    }

    #[test]
    fn working_runs_faster_than_idle() {
        let mut pet = companion();
        pet.feel(Mood::Working);
        assert_eq!(pet.next_frame_in(), Duration::from_millis(125));
        assert!(pet.advance(Duration::from_millis(125)));
        assert_eq!(pet.sprite(), &sprite(8));
    }

    #[test]
    fn a_new_mood_restarts_and_the_same_mood_does_not() {
        let mut pet = companion();
        pet.feel(Mood::Working);
        pet.advance(Duration::from_millis(125));
        pet.feel(Mood::Working);
        assert_eq!(pet.sprite(), &sprite(8));
        pet.feel(Mood::Idle);
        assert_eq!(pet.sprite(), &sprite(1));
    }

    #[test]
    fn a_mood_without_a_clip_falls_back_to_idle() {
        let mut pet = companion();
        pet.feel(Mood::Failed);
        assert_eq!(pet.mood(), Mood::Failed);
        assert_eq!(pet.sprite(), &sprite(1));
    }

    #[test]
    fn bounds_hold_every_frame() {
        let tall = Sprite::new(1, 5, vec![0; 20]).unwrap();
        let wide = Sprite::new(3, 1, vec![0; 12]).unwrap();
        let clips = HashMap::from([(Mood::Idle, Clip::new(vec![tall, wide]).unwrap())]);
        assert_eq!(Pet::new("x", clips).unwrap().bounds(), (3, 5));
    }
}
