//! What a pet does on its own while nothing is going on. Time and chance are passed in, so
//! every rule here is tested without a stage. A pet takes every behaviour its sheet has rows
//! for and skips the rest, and frame numbers come from each row's own length, because
//! community sheets vary.
use crate::{Mood, Pet, Sprite};
use std::time::Duration;

const GLANCE_EVERY_MS: (u64, u64) = (6_000, 20_000);
const GLANCE_LENGTH: Duration = Duration::from_millis(1500);
const GLANCE_FRAMES: usize = 4;
const STROLL_EVERY_MS: (u64, u64) = (60_000, 240_000);
const STROLL_PIXELS: (u64, u64) = (60, 200);
const STROLL_SPEED: f32 = 40.0;
const STROLL_FPS: u32 = 8;
/// How often a walking pet moves: 2 pixels at a time at this speed.
const STEP_EVERY: Duration = Duration::from_millis(50);
/// The most walking a late tick can make up for, so a stall does not look like a teleport.
const LONGEST_STRIDE: Duration = Duration::from_millis(250);
const DOZE_AFTER: Duration = Duration::from_secs(300);
const DOZE_FPS: u32 = 3;
/// A held frame is never redrawn; the stage only comes back this often to hear reports.
const DOZE_LISTEN_EVERY: Duration = Duration::from_millis(250);
/// How far the pointer may wander during a press that still counts as a click.
const CLICK_SLOP: u32 = 3;

/// A source of chance: each call returns any `u64`.
pub type Dice = Box<dyn FnMut() -> u64 + Send>;

/// What a stage saw the pointer do to the pet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pointer {
    Entered,
    /// It left the pet. During a press, that means the grab was lost.
    Left,
    Pressed,
    /// The pet was dragged this many pixels, on top of any before.
    Dragged(u32),
    Released,
}

#[derive(Debug, Clone, Copy)]
enum Act {
    Still,
    Glancing { row: usize, first: usize, frames: usize },
    Strolling { rightward: bool, pixels_left: u32, owed: f32, blocked: bool },
    Dozing,
    Waving,
    Startled,
}

#[derive(Debug, Clone, Copy)]
struct Press {
    travelled: u32,
    /// A press that woke the pet was spent on that.
    waves: bool,
}

/// The state machine of an idle pet. It answers "is something other than the mood's clip on
/// screen?", and asks a stage that can move the pet to walk.
pub struct Life {
    dice: Dice,
    act: Act,
    frame: usize,
    since_frame: Duration,
    until_glance: Duration,
    until_stroll: Duration,
    /// Time since the last report or press.
    quiet: Duration,
    rooted: bool,
    idle: bool,
    press: Option<Press>,
    /// What is on screen changed and the stage has not been told.
    dirty: bool,
    asked: i32,
}

impl Life {
    pub fn new(dice: Dice) -> Self {
        let mut life = Self {
            dice,
            act: Act::Still,
            frame: 0,
            since_frame: Duration::ZERO,
            until_glance: Duration::ZERO,
            until_stroll: Duration::ZERO,
            quiet: Duration::ZERO,
            rooted: false,
            idle: true,
            press: None,
            dirty: false,
            asked: 0,
        };
        life.until_glance = life.roll_wait(GLANCE_EVERY_MS);
        life.until_stroll = life.roll_wait(STROLL_EVERY_MS);
        life
    }

    /// Never strolls: everything else stays.
    pub fn rooted(mut self) -> Self {
        self.rooted = true;
        self
    }

    /// A value in `low..=high`.
    fn roll(&mut self, (low, high): (u64, u64)) -> u64 {
        let span = u128::from(high - low + 1);
        low + ((u128::from((self.dice)()) * span) >> 64) as u64
    }

    fn roll_wait(&mut self, range: (u64, u64)) -> Duration {
        Duration::from_millis(self.roll(range))
    }

    /// Advances time. `idle` says whether the pet's mood is idle: any other mood wins over
    /// everything here. Returns whether what is on screen changed.
    pub fn advance(&mut self, pet: &Pet, elapsed: Duration, idle: bool) -> bool {
        self.idle = idle;
        self.quiet += elapsed;
        if !idle && !matches!(self.act, Act::Still | Act::Startled) {
            self.finish();
        }
        self.play(pet, elapsed);
        if idle && self.press.is_none() {
            // A glance is short, and must not push back the next stroll.
            if matches!(self.act, Act::Still | Act::Glancing { .. }) {
                self.until_stroll = self.until_stroll.saturating_sub(elapsed);
            }
            if matches!(self.act, Act::Still) {
                self.until_glance = self.until_glance.saturating_sub(elapsed);
                self.begin_something(pet);
            }
        }
        std::mem::take(&mut self.dirty)
    }

    /// A report this pet cares about arrived. It interrupts everything at once, and a dozing
    /// pet wakes with a start.
    pub fn hear(&mut self, pet: &Pet) {
        self.quiet = Duration::ZERO;
        match self.act {
            Act::Dozing => self.wake(pet),
            Act::Still | Act::Startled => {}
            _ => self.finish(),
        }
    }

    /// Tells what the pointer did. Returns whether what is on screen changed.
    pub fn pointer(&mut self, pet: &Pet, event: Pointer) -> bool {
        match event {
            Pointer::Entered => {
                if self.idle && self.press.is_none() && matches!(self.act, Act::Still) {
                    self.begin_glance(pet);
                }
            }
            Pointer::Left => self.press = None,
            Pointer::Pressed => {
                self.quiet = Duration::ZERO;
                let waves = !matches!(self.act, Act::Dozing | Act::Startled);
                match self.act {
                    Act::Dozing => self.wake(pet),
                    Act::Still | Act::Startled => {}
                    _ => self.finish(),
                }
                self.press = Some(Press { travelled: 0, waves });
            }
            Pointer::Dragged(distance) => {
                if let Some(press) = &mut self.press {
                    press.travelled = press.travelled.saturating_add(distance);
                }
            }
            Pointer::Released => {
                // Being held is not being left alone.
                self.quiet = Duration::ZERO;
                if let Some(press) = self.press.take() {
                    let clicked = press.waves && press.travelled <= CLICK_SLOP;
                    if clicked && self.idle && pet.own_clip(Mood::Done).is_some() {
                        self.begin(Act::Waving);
                    }
                }
            }
        }
        std::mem::take(&mut self.dirty)
    }

    /// What to show instead of the mood's clip, if anything.
    pub fn sprite<'a>(&self, pet: &'a Pet) -> Option<&'a Sprite> {
        let moves = pet.moves();
        match self.act {
            Act::Still => None,
            Act::Glancing { row, first, .. } => Some(moves.looks.get(row)?.frame(first + self.frame)),
            Act::Strolling { rightward, .. } => {
                let (right, left) = moves.running.as_ref()?;
                Some(if rightward { right } else { left }.frame(self.frame))
            }
            Act::Dozing => Some(pet.own_clip(Mood::Failed)?.frame(self.frame)),
            Act::Waving => Some(pet.own_clip(Mood::Done)?.frame(self.frame)),
            Act::Startled => Some(pet.own_clip(Mood::Celebrating)?.frame(self.frame)),
        }
    }

    /// How long until this wants the stage back. `None` leaves it to the mood's clip.
    pub fn next_in(&self, pet: &Pet) -> Option<Duration> {
        if self.dirty {
            return Some(Duration::ZERO);
        }
        let frame_due = self.frame_time().saturating_sub(self.since_frame);
        match self.act {
            Act::Still => None,
            Act::Strolling { .. } => Some(frame_due.min(STEP_EVERY)),
            Act::Dozing if self.frame >= doze_hold(pet) => Some(DOZE_LISTEN_EVERY),
            _ => Some(frame_due),
        }
    }

    /// The step a walking pet wants to take now, in pixels: positive is to the right. A stage
    /// answers with [`Life::walked`].
    pub fn step(&mut self) -> Option<i32> {
        let Act::Strolling { rightward, pixels_left, ref mut owed, .. } = self.act else { return None };
        let pixels = (*owed as u32).min(pixels_left);
        if pixels == 0 {
            return None;
        }
        *owed -= pixels as f32;
        self.asked = if rightward { pixels as i32 } else { -(pixels as i32) };
        Some(self.asked)
    }

    /// How far the last step really moved the pet. Less than asked means it reached an edge,
    /// so it turns around; with no room either way, it stops.
    pub fn walked(&mut self, moved: i32) {
        let asked = self.asked;
        let Act::Strolling { rightward, pixels_left, blocked, owed } = &mut self.act else { return };
        let forward = if (moved > 0) == (asked > 0) { moved.unsigned_abs() } else { 0 };
        *pixels_left = pixels_left.saturating_sub(forward);
        let mut done = *pixels_left == 0;
        if forward < asked.unsigned_abs() {
            done |= *blocked && forward == 0;
            *blocked = true;
            *rightward = !*rightward;
            *owed = 0.0;
            self.dirty = true;
        } else if forward > 0 {
            *blocked = false;
        }
        if done {
            self.finish();
        }
    }

    fn frame_time(&self) -> Duration {
        match self.act {
            Act::Still => Duration::MAX,
            Act::Glancing { frames, .. } => GLANCE_LENGTH / frames as u32,
            Act::Strolling { .. } => Duration::from_secs(1) / STROLL_FPS,
            Act::Dozing => Duration::from_secs(1) / DOZE_FPS,
            Act::Waving => Duration::from_secs(1) / Mood::Done.fps(),
            Act::Startled => Duration::from_secs(1) / Mood::Celebrating.fps(),
        }
    }

    fn play(&mut self, pet: &Pet, elapsed: Duration) {
        if matches!(self.act, Act::Still) {
            return;
        }
        if let Act::Strolling { owed, .. } = &mut self.act {
            *owed += STROLL_SPEED * elapsed.min(LONGEST_STRIDE).as_secs_f32();
        }
        self.since_frame += elapsed;
        let frame_time = self.frame_time();
        while self.since_frame >= frame_time {
            self.since_frame -= frame_time;
            let plays_to = match self.act {
                Act::Still => return,
                Act::Glancing { frames, .. } => frames,
                Act::Strolling { .. } => usize::MAX,
                Act::Dozing => {
                    if self.frame < doze_hold(pet) {
                        self.frame += 1;
                        self.dirty = true;
                    }
                    continue;
                }
                Act::Waving => pet.own_clip(Mood::Done).map_or(0, |clip| clip.len()),
                Act::Startled => pet.own_clip(Mood::Celebrating).map_or(0, |clip| clip.len()),
            };
            self.frame += 1;
            self.dirty = true;
            if self.frame >= plays_to {
                self.finish();
            }
        }
    }

    fn begin_something(&mut self, pet: &Pet) {
        if self.quiet >= DOZE_AFTER && pet.own_clip(Mood::Failed).is_some() {
            self.begin(Act::Dozing);
        } else if !self.rooted && self.until_stroll.is_zero() && pet.moves().running.is_some() {
            let pixels_left = self.roll(STROLL_PIXELS) as u32;
            let rightward = (self.dice)() & 1 == 0;
            self.begin(Act::Strolling { rightward, pixels_left, owed: 0.0, blocked: false });
        } else if self.until_glance.is_zero() {
            self.begin_glance(pet);
        }
    }

    /// A few frames in a row of one head turn, however long its row happens to be.
    fn begin_glance(&mut self, pet: &Pet) {
        let looks = &pet.moves().looks;
        if looks.is_empty() {
            return;
        }
        let row = self.roll((0, looks.len() as u64 - 1)) as usize;
        let length = looks[row].len();
        let frames = GLANCE_FRAMES.min(length);
        let first = self.roll((0, (length - frames) as u64)) as usize;
        self.begin(Act::Glancing { row, first, frames });
    }

    fn begin(&mut self, act: Act) {
        self.act = act;
        self.frame = 0;
        self.since_frame = Duration::ZERO;
        self.dirty = true;
    }

    fn finish(&mut self) {
        match self.act {
            Act::Glancing { .. } => self.until_glance = self.roll_wait(GLANCE_EVERY_MS),
            Act::Strolling { .. } => self.until_stroll = self.roll_wait(STROLL_EVERY_MS),
            _ => {}
        }
        self.begin(Act::Still);
    }

    /// Waking starts the waits over, and jumps if the sheet can.
    fn wake(&mut self, pet: &Pet) {
        self.until_glance = self.roll_wait(GLANCE_EVERY_MS);
        self.until_stroll = self.roll_wait(STROLL_EVERY_MS);
        let jump = pet.own_clip(Mood::Celebrating).is_some();
        self.begin(if jump { Act::Startled } else { Act::Still });
    }
}

/// The frame of the failed row a pet dozes on: halfway through, where a standard sheet's
/// failed animation is at its lowest before it recovers. It depends only on the row's length.
fn doze_hold(pet: &Pet) -> usize {
    pet.own_clip(Mood::Failed).map_or(0, |clip| clip.len() / 2)
}
