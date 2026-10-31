//! Use cases and the ports the outside world plugs into. To support something new:
//! a new pet format implements [`PetSource`], a new place to show the pet (an OS, a
//! terminal, an editor panel) implements [`Stage`], and a new way of hearing about
//! activity implements [`ActivityFeed`], and a way of writing text implements [`Captioner`].
use familiard_domain::{caption_mood, caption_moods, clean_label, fit_caption, Activity, Companion, Mood, Pet, Report, Sprite};
use std::error::Error;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

pub type Failure = Box<dyn Error + Send + Sync>;

/// How wide a caption may get, in pixels.
const MAX_CAPTION_WIDTH: u32 = 240;
/// Pixels between the caption and the tallest frame of the pet.
const CAPTION_GAP: u32 = 4;

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

/// Driven port: turns text into a picture, so that no stage has to know about fonts.
pub trait Captioner {
    /// The size `draw` would produce for this text.
    fn size_of(&self, text: &str) -> (u32, u32);
    fn draw(&self, text: &str) -> Sprite;
}

/// The label over a pet's head, already drawn for every state: its words are known up front
/// and a font is heavy, so nothing has to stay around to write them.
struct Caption {
    faces: Vec<(Mood, String, Sprite)>,
    showing: usize,
}

impl Caption {
    fn new(label: &str, captioner: &dyn Captioner) -> Self {
        let faces = caption_moods().map(|mood| {
            let text = fit_caption(label, mood, MAX_CAPTION_WIDTH, |text| captioner.size_of(text).0);
            let sprite = captioner.draw(&text);
            (mood, text, sprite)
        });
        Self { faces: faces.into(), showing: 0 }
    }

    fn text(&self) -> &str {
        &self.faces[self.showing].1
    }

    fn sprite(&self) -> &Sprite {
        &self.faces[self.showing].2
    }

    /// Returns whether the words changed.
    fn show(&mut self, mood: Mood) -> bool {
        let wanted = caption_mood(mood);
        let face = self.faces.iter().position(|(mood, ..)| *mood == wanted).unwrap_or(0);
        let changed = face != self.showing;
        self.showing = face;
        changed
    }
}

/// Driver port for stages: runs the companion, without knowing about any display.
pub struct Player {
    companion: Companion,
    activity: Activity,
    inbox: Receiver<Report>,
    caption: Option<Caption>,
    bounds: (u32, u32),
}

impl Player {
    pub fn new(pet: Pet, inbox: Receiver<Report>) -> Self {
        let bounds = pet.bounds();
        Self { companion: Companion::new(pet), activity: Activity::default(), inbox, caption: None, bounds }
    }

    /// Follows only the next session to report from `folder` or below it, and leaves when
    /// that session ends.
    pub fn following(mut self, folder: impl Into<String>) -> Self {
        self.activity = Activity::following(folder);
        self
    }

    /// Follows only this exact session, and leaves when it ends.
    pub fn following_session(mut self, session: impl Into<String>) -> Self {
        self.activity = Activity::following_session(session);
        self
    }

    /// Shows `label` and the pet's state above its head. The captioner is only needed here:
    /// every caption this pet can show is drawn now, so a stage never waits for a font and
    /// the font does not stay in memory. The box also grows here, once, to hold the widest
    /// of them. A blank label shows nothing.
    pub fn captioned(mut self, label: &str, captioner: &dyn Captioner) -> Self {
        let label = clean_label(label);
        if label.is_empty() {
            return self;
        }
        let caption = Caption::new(&label, captioner);
        let sizes = caption.faces.iter().map(|(.., sprite)| (sprite.width(), sprite.height()));
        let (width, height) = sizes.fold((0, 0), |(w, h), size| (w.max(size.0), h.max(size.1)));
        let (pet_width, pet_height) = self.companion.pet().bounds();
        self.bounds = (width.max(pet_width), pet_height + CAPTION_GAP + height);
        self.caption = Some(caption);
        self
    }

    /// The box a stage needs so that no frame or caption ever has to resize it.
    pub fn bounds(&self) -> (u32, u32) {
        self.bounds
    }

    /// The box the frames alone need, for a stage that does not draw the caption's pixels.
    pub fn pet_bounds(&self) -> (u32, u32) {
        self.companion.pet().bounds()
    }

    /// The caption's words, for a stage that can print text.
    pub fn caption_text(&self) -> Option<&str> {
        self.caption.as_ref().map(Caption::text)
    }

    /// Whether the pet should go: its session ended. A stage ends its loop when it sees this.
    pub fn leaving(&self) -> bool {
        self.activity.finished()
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
        let new_caption = self.caption.as_mut().is_some_and(|caption| caption.show(mood));
        (new_mood || new_frame || new_caption).then(|| self.companion.sprite())
    }

    /// Paints the whole picture into a canvas of [`Player::bounds`], as premultiplied BGRA.
    /// Returns where the sprite landed, which is what an input region needs.
    pub fn paint(&self, canvas: &mut [u8]) -> (u32, u32) {
        let at = paint_bgra(canvas, self.bounds, self.companion.sprite());
        if let Some(caption) = &self.caption {
            paint_caption_bgra(canvas, self.bounds, caption.sprite());
        }
        at
    }

    pub fn next_frame_in(&self) -> Duration {
        self.companion.next_frame_in()
    }
}

/// Paints a sprite bottom-centered as premultiplied BGRA, the format desktop compositors
/// want, clearing everything else. Returns where the sprite landed inside the box.
pub fn paint_bgra(canvas: &mut [u8], box_size: (u32, u32), sprite: &Sprite) -> (u32, u32) {
    canvas.fill(0);
    let (width, height) = box_size;
    let left = width.saturating_sub(sprite.width()) / 2;
    let top = height.saturating_sub(sprite.height());
    for (y, row) in sprite.rgba().chunks_exact(sprite.width() as usize * 4).enumerate() {
        let start = ((top as usize + y) * width as usize + left as usize) * 4;
        for (pixel, out) in row.chunks_exact(4).zip(canvas[start..].chunks_exact_mut(4)) {
            let alpha = pixel[3] as u32;
            let scale = |channel: u8| ((channel as u32 * alpha + 127) / 255) as u8;
            out.copy_from_slice(&[scale(pixel[2]), scale(pixel[1]), scale(pixel[0]), pixel[3]]);
        }
    }
    (left, top)
}

/// Composites a caption over what is already on the canvas, at the top center of the box.
/// Returns where it landed.
pub fn paint_caption_bgra(canvas: &mut [u8], box_size: (u32, u32), caption: &Sprite) -> (u32, u32) {
    let (width, height) = box_size;
    let left = width.saturating_sub(caption.width()) / 2;
    let visible = caption.width().min(width) as usize * 4;
    for (y, row) in caption.rgba().chunks_exact(caption.width() as usize * 4).take(height as usize).enumerate() {
        let start = (y * width as usize + left as usize) * 4;
        for (pixel, out) in row[..visible].chunks_exact(4).zip(canvas[start..].chunks_exact_mut(4)) {
            let alpha = pixel[3] as u32;
            let over = |source: u8, below: u8| {
                ((source as u32 * alpha + 127) / 255 + (below as u32 * (255 - alpha) + 127) / 255).min(255) as u8
            };
            out.copy_from_slice(&[
                over(pixel[2], out[0]),
                over(pixel[1], out[1]),
                over(pixel[0], out[2]),
                over(255, out[3]),
            ]);
        }
    }
    (left, 0)
}

/// The main use case: bring a pet to a stage. `prepare` sets what makes this pet particular,
/// such as which session it follows and what its caption says.
pub fn summon(
    source: &dyn PetSource,
    stage: Box<dyn Stage>,
    inbox: Receiver<Report>,
    prepare: impl FnOnce(Player) -> Player,
) -> Result<(), Failure> {
    stage.perform(prepare(Player::new(source.load()?, inbox)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use familiard_domain::{Clip, Mood, Signal};
    use std::collections::HashMap;
    use std::sync::mpsc::channel;
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::mpsc::Sender;
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
        summon(&FixedPet, Box::new(RecordingStage(shown.clone())), channel().1, |player| player).unwrap();
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
        outbox.send(Report { session: "a".into(), signal: Signal::Working, folder: None }).unwrap();
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
    fn paints_premultiplied_bgra_at_the_bottom_center() {
        let sprite = Sprite::new(1, 1, vec![200, 100, 50, 128]).unwrap();
        let mut canvas = vec![7u8; 3 * 2 * 4];
        assert_eq!(paint_bgra(&mut canvas, (3, 2), &sprite), (1, 1));
        let at = (3 + 1) * 4;
        assert_eq!(&canvas[at..at + 4], &[25, 50, 100, 128]);
        assert!(canvas[..at].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn a_broken_source_never_reaches_the_stage() {
        let shown = Arc::new(Mutex::new(Vec::new()));
        let error = summon(&BrokenPet, Box::new(RecordingStage(shown.clone())), channel().1, |player| player).unwrap_err();
        assert_eq!(error.to_string(), "no such pet");
        assert!(shown.lock().unwrap().is_empty());
    }

    /// One pixel wide per character and 10 tall, in a solid color, counting what it draws.
    struct CountingCaptioner(Rc<Cell<u32>>);
    impl Captioner for CountingCaptioner {
        fn size_of(&self, text: &str) -> (u32, u32) {
            (text.chars().count() as u32, 10)
        }
        fn draw(&self, text: &str) -> Sprite {
            self.0.set(self.0.get() + 1);
            let (width, height) = self.size_of(text);
            Sprite::new(width, height, [10, 20, 30, 255].repeat((width * height) as usize)).unwrap()
        }
    }

    fn pet_of_size(width: u32, height: u32) -> Pet {
        let frame = Sprite::new(width, height, [200, 100, 50, 255].repeat((width * height) as usize)).unwrap();
        Pet::new("p", HashMap::from([(Mood::Idle, Clip::new(vec![frame]).unwrap())])).unwrap()
    }

    fn captioned(label: &str, pet: Pet) -> (Player, Sender<Report>, Rc<Cell<u32>>) {
        let (outbox, inbox) = channel();
        let draws = Rc::new(Cell::new(0));
        (Player::new(pet, inbox).captioned(label, &CountingCaptioner(draws.clone())), outbox, draws)
    }

    fn says(session: &str, signal: Signal) -> Report {
        Report { session: session.into(), signal, folder: Some("/work/app".into()) }
    }

    #[test]
    fn a_caption_follows_the_state_and_a_change_of_words_is_a_change_on_screen() {
        let (mut player, outbox, _) = captioned("Bug", pet_of_size(1, 1));
        assert_eq!(player.caption_text(), Some("Bug"));
        for (signal, text) in [
            (Signal::Working, "Bug \u{b7} working"),
            (Signal::Waiting, "Bug \u{b7} needs you"),
            (Signal::Done, "Bug \u{b7} done"),
        ] {
            outbox.send(says("a", signal)).unwrap();
            assert!(player.tick(Duration::ZERO).is_some());
            assert_eq!(player.caption_text(), Some(text));
        }
        outbox.send(says("a", Signal::Ended)).unwrap();
        assert!(player.tick(Duration::ZERO).is_some());
        assert_eq!(player.caption_text(), Some("Bug"));
    }

    #[test]
    fn a_pet_without_a_label_has_no_caption_and_keeps_its_own_box() {
        let (outbox, inbox) = channel();
        let mut player = Player::new(pet_of_size(3, 2), inbox);
        outbox.send(says("a", Signal::Working)).unwrap();
        player.tick(Duration::ZERO);
        assert_eq!(player.caption_text(), None);
        assert_eq!(player.bounds(), (3, 2));
        let (blank, _, _) = captioned(" \n ", pet_of_size(3, 2));
        assert_eq!(blank.caption_text(), None);
        assert_eq!(blank.bounds(), (3, 2));
    }

    #[test]
    fn each_caption_is_drawn_once_up_front_and_never_while_running() {
        let (mut player, outbox, draws) = captioned("Bug", pet_of_size(1, 1));
        assert_eq!(draws.get(), 4);
        for signal in [Signal::Working, Signal::Waiting, Signal::Done, Signal::Ended] {
            outbox.send(says("a", signal)).unwrap();
            for _ in 0..20 {
                player.tick(Duration::from_millis(400));
            }
        }
        assert_eq!(draws.get(), 4);
    }

    #[test]
    fn a_caption_changes_only_when_its_words_do() {
        let (mut player, outbox, _) = captioned("Bug", pet_of_size(1, 1));
        outbox.send(says("a", Signal::Done)).unwrap();
        player.tick(Duration::ZERO);
        assert_eq!(player.companion.mood(), Mood::Celebrating);
        assert_eq!(player.caption_text(), Some("Bug \u{b7} done"));
        let caption = player.caption.as_mut().unwrap();
        assert!(!caption.show(Mood::Done), "celebrating and settling read the same");
        assert!(caption.show(Mood::Working));
        assert!(!caption.show(Mood::Working));
    }

    #[test]
    fn the_box_holds_the_widest_caption_from_the_start_and_never_changes() {
        // "Bug · needs you" is the widest, at 15 characters.
        let (mut player, outbox, _) = captioned("Bug", pet_of_size(4, 6));
        assert_eq!(player.bounds(), (15, 6 + CAPTION_GAP + 10));
        assert_eq!(player.pet_bounds(), (4, 6));
        for signal in [Signal::Working, Signal::Waiting, Signal::Done, Signal::Ended] {
            outbox.send(says("a", signal)).unwrap();
            player.tick(Duration::ZERO);
            assert_eq!(player.bounds(), (15, 6 + CAPTION_GAP + 10));
        }
        let (wide_pet, _, _) = captioned("Bug", pet_of_size(100, 6));
        assert_eq!(wide_pet.bounds().0, 100);
    }

    #[test]
    fn a_long_label_is_cut_to_the_maximum_width_in_every_state() {
        let (mut player, outbox, _) = captioned(&"x".repeat(400), pet_of_size(1, 1));
        assert_eq!(player.bounds().0, MAX_CAPTION_WIDTH);
        for signal in [Signal::Working, Signal::Waiting, Signal::Done] {
            outbox.send(says("a", signal)).unwrap();
            player.tick(Duration::ZERO);
            let text = player.caption_text().unwrap();
            assert_eq!(text.chars().count() as u32, MAX_CAPTION_WIDTH, "{text}");
            assert!(text.contains('\u{2026}'), "{text}");
        }
    }

    #[test]
    fn the_caption_sits_top_center_and_the_sprite_stays_on_the_floor() {
        // A 2x1 pet under the 15x10 pill of "Bug · needs you": the box is 15x15.
        let (mut player, outbox, _) = captioned("Bug", pet_of_size(2, 1));
        outbox.send(says("a", Signal::Waiting)).unwrap();
        player.tick(Duration::ZERO);
        let (width, height) = player.bounds();
        assert_eq!((width, height), (15, 15));
        let mut canvas = vec![9u8; (width * height * 4) as usize];
        assert_eq!(player.paint(&mut canvas), (6, 14));
        let at = |x: u32, y: u32| {
            let start = ((y * width + x) * 4) as usize;
            canvas[start..start + 4].to_vec()
        };
        assert_eq!(at(0, 0), [30, 20, 10, 255], "the pill, in BGRA");
        assert_eq!(at(14, 9), [30, 20, 10, 255]);
        assert_eq!(at(0, 10), [0, 0, 0, 0], "the gap");
        assert_eq!(at(6, 14), [50, 100, 200, 255], "the sprite");
        assert_eq!(at(5, 14), [0, 0, 0, 0]);
    }

    #[test]
    fn a_short_caption_is_centered_over_a_wide_pet() {
        let caption = Sprite::new(2, 1, [255, 255, 255, 255].repeat(2)).unwrap();
        let mut canvas = vec![0u8; 6 * 4];
        assert_eq!(paint_caption_bgra(&mut canvas, (6, 1), &caption), (2, 0));
        assert_eq!(&canvas[8..16], &[255; 8]);
        assert!(canvas[..8].iter().chain(&canvas[16..]).all(|byte| *byte == 0));
    }

    #[test]
    fn the_caption_composites_over_the_canvas_keeping_it_premultiplied() {
        let half_red = Sprite::new(1, 1, vec![255, 0, 0, 128]).unwrap();
        let mut canvas = vec![100, 100, 100, 200];
        paint_caption_bgra(&mut canvas, (1, 1), &half_red);
        // Blue and green keep 127/255 of what was below; red adds 128; alpha gets the rest.
        assert_eq!(canvas, [50, 50, 178, 228]);
    }

    #[test]
    fn a_bound_pet_leaves_when_its_session_ends_and_only_then() {
        let (outbox, inbox) = channel();
        let mut player = Player::new(pet_of_size(1, 1), inbox).following("/work/app");
        assert!(!player.leaving());
        outbox.send(Report { session: "other".into(), signal: Signal::Working, folder: Some("/else".into()) }).unwrap();
        outbox.send(says("mine", Signal::Working)).unwrap();
        player.tick(Duration::ZERO);
        assert_eq!(player.companion.mood(), Mood::Working);
        assert!(!player.leaving());
        player.tick(Duration::from_secs(3600));
        assert!(!player.leaving(), "being quiet is not leaving");
        outbox.send(says("other", Signal::Ended)).unwrap();
        player.tick(Duration::ZERO);
        assert!(!player.leaving());
        outbox.send(says("mine", Signal::Ended)).unwrap();
        player.tick(Duration::ZERO);
        assert!(player.leaving());
    }

    #[test]
    fn a_global_pet_never_leaves() {
        let (outbox, inbox) = channel();
        let mut player = Player::new(pet_of_size(1, 1), inbox);
        outbox.send(says("a", Signal::Working)).unwrap();
        outbox.send(says("a", Signal::Ended)).unwrap();
        player.tick(Duration::ZERO);
        assert!(!player.leaving());
    }

    #[test]
    fn summon_lets_the_caller_prepare_the_player() {
        struct Leaves(Arc<Mutex<Option<bool>>>);
        impl Stage for Leaves {
            fn perform(self: Box<Self>, player: Player) -> Result<(), Failure> {
                *self.0.lock().unwrap() = Some(player.caption_text() == Some("Bug"));
                Ok(())
            }
        }
        let seen = Arc::new(Mutex::new(None));
        let captioner = CountingCaptioner(Rc::new(Cell::new(0)));
        summon(&FixedPet, Box::new(Leaves(seen.clone())), channel().1, |player| player.captioned("Bug", &captioner)).unwrap();
        assert_eq!(*seen.lock().unwrap(), Some(true));
    }
}
