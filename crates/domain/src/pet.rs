use crate::Sprite;
use std::collections::HashMap;
use std::fmt;

/// What the pet is expressing. Moods without their own clip fall back to `Idle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mood {
    Idle,
    Working,
    Waiting,
    Done,
    Celebrating,
    Failed,
    Reviewing,
}

impl Mood {
    /// Frames per second for this mood.
    pub fn fps(self) -> u32 {
        match self {
            Mood::Idle | Mood::Done => 3,
            Mood::Waiting | Mood::Failed | Mood::Reviewing => 4,
            Mood::Working | Mood::Celebrating => 8,
        }
    }
}

/// A looping sequence of frames. Never empty.
#[derive(Debug, Clone)]
pub struct Clip {
    frames: Vec<Sprite>,
}

impl Clip {
    pub fn new(frames: Vec<Sprite>) -> Option<Self> {
        (!frames.is_empty()).then_some(Self { frames })
    }

    pub fn frame(&self, index: usize) -> &Sprite {
        &self.frames[index % self.frames.len()]
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PetError {
    MissingIdleClip,
}

impl fmt::Display for PetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PetError::MissingIdleClip => write!(f, "a pet needs at least an idle animation"),
        }
    }
}

impl std::error::Error for PetError {}

/// What a pet can do on its own beyond its moods. A sheet that lacks the rows has none of
/// this, and the behaviour that needs them is skipped.
#[derive(Debug, Clone, Default)]
pub struct Moves {
    /// Running right and running left: a stroll needs both, to turn around at an edge.
    pub running: Option<(Clip, Clip)>,
    /// Rows of head turns.
    pub looks: Vec<Clip>,
}

/// A character: its name, an animation per mood, and what it does on its own.
#[derive(Debug, Clone)]
pub struct Pet {
    name: String,
    clips: HashMap<Mood, Clip>,
    moves: Moves,
}

impl Pet {
    pub fn new(name: impl Into<String>, clips: HashMap<Mood, Clip>) -> Result<Self, PetError> {
        if !clips.contains_key(&Mood::Idle) {
            return Err(PetError::MissingIdleClip);
        }
        Ok(Self { name: name.into(), clips, moves: Moves::default() })
    }

    pub fn with_moves(mut self, moves: Moves) -> Self {
        self.moves = moves;
        self
    }

    pub fn moves(&self) -> &Moves {
        &self.moves
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn clip(&self, mood: Mood) -> &Clip {
        self.clips.get(&mood).unwrap_or_else(|| &self.clips[&Mood::Idle])
    }

    /// The clip of exactly this mood, without falling back to idle.
    pub fn own_clip(&self, mood: Mood) -> Option<&Clip> {
        self.clips.get(&mood)
    }

    /// The smallest box that holds every frame, so a stage never has to resize.
    pub fn bounds(&self) -> (u32, u32) {
        let running = self.moves.running.iter().flat_map(|(right, left)| [right, left]);
        let clips = self.clips.values().chain(running).chain(&self.moves.looks);
        let frames = clips.flat_map(|clip| clip.frames.iter());
        frames.fold((0, 0), |(w, h), sprite| (w.max(sprite.width()), h.max(sprite.height())))
    }
}
