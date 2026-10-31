//! Every timing rule of a pet's idle life, through the public interface, with no stage.
use familiard_domain::{Clip, Dice, Life, Mood, Moves, Pet, Pointer, Sprite};
use std::collections::HashMap;
use std::time::Duration;

const IDLE: u8 = 1;
const WAVE: u8 = 30;
const JUMP: u8 = 40;
const DOZE: u8 = 50;
const RIGHT: u8 = 10;
const LEFT: u8 = 20;
const LOOKS: u8 = 90;

/// Frames tell themselves apart by their first byte: the row's base plus the frame's index.
fn clip(base: u8, frames: u8) -> Clip {
    Clip::new((0..frames).map(|at| Sprite::new(1, 1, vec![base + at, 0, 0, 255]).unwrap()).collect()).unwrap()
}

/// A sheet with this many frames in the rows that matter here. Zero means the row is missing.
fn pet(doze: u8, wave: u8, jump: u8, run: u8, looks: u8) -> Pet {
    let mut clips = HashMap::from([(Mood::Idle, clip(IDLE, 1))]);
    if doze > 0 {
        clips.insert(Mood::Failed, clip(DOZE, doze));
    }
    if wave > 0 {
        clips.insert(Mood::Done, clip(WAVE, wave));
    }
    if jump > 0 {
        clips.insert(Mood::Celebrating, clip(JUMP, jump));
    }
    let running = (run > 0).then(|| (clip(RIGHT, run), clip(LEFT, run)));
    let looks = if looks > 0 { vec![clip(LOOKS, looks), clip(LOOKS + 40, looks)] } else { vec![] };
    Pet::new("p", clips).unwrap().with_moves(Moves { running, looks })
}

fn full() -> Pet {
    pet(8, 4, 5, 8, 8)
}

/// A v1 standard sheet: nine rows, so no looks.
fn v1() -> Pet {
    pet(8, 4, 5, 8, 0)
}

fn bare() -> Pet {
    pet(0, 0, 0, 0, 0)
}

/// A dice always landing on the same value: 0 picks the low end of every range.
fn fixed(value: u64) -> Dice {
    Box::new(move || value)
}

fn secs(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

fn shown(life: &Life, pet: &Pet) -> Option<u8> {
    life.sprite(pet).map(|sprite| sprite.rgba()[0])
}

/// A pet with the longest glance wait out of the way, for tests about strolling and dozing.
fn no_glances(run: u8) -> Pet {
    pet(8, 4, 5, run, 0)
}

fn into_a_stroll(pet: &Pet, dice: u64) -> Life {
    let mut life = Life::new(fixed(dice));
    life.advance(pet, secs(240), true);
    assert!(life.sprite(pet).is_some(), "the stroll should have started");
    life
}

fn dozing(pet: &Pet) -> Life {
    let mut life = Life::new(fixed(0)).rooted();
    life.advance(pet, secs(300), true);
    life.advance(pet, secs(10), true);
    assert_eq!(shown(&life, pet), Some(DOZE + 4));
    life
}

// S1: glance

#[test]
fn the_first_glance_comes_between_6_and_20_seconds() {
    let pet = pet(0, 0, 0, 0, 8);
    let mut soon = Life::new(fixed(0));
    soon.advance(&pet, ms(5900), true);
    assert_eq!(shown(&soon, &pet), None);
    soon.advance(&pet, ms(100), true);
    assert!(shown(&soon, &pet).is_some());

    let mut late = Life::new(fixed(u64::MAX));
    late.advance(&pet, ms(19_900), true);
    assert_eq!(shown(&late, &pet), None);
    late.advance(&pet, ms(1100), true);
    assert!(shown(&late, &pet).is_some());
}

#[test]
fn a_glance_shows_a_few_frames_of_one_head_turn_for_about_a_second_and_a_half_then_idle() {
    let pet = pet(0, 0, 0, 0, 8);
    let mut life = Life::new(fixed(0));
    life.advance(&pet, secs(6), true);
    let mut seen = vec![shown(&life, &pet).unwrap()];
    let mut passed = Duration::ZERO;
    while life.sprite(&pet).is_some() {
        if life.advance(&pet, ms(25), true) {
            seen.extend(shown(&life, &pet));
        }
        passed += ms(25);
        assert!(passed < secs(3), "the glance never ended");
    }
    assert_eq!(seen, [LOOKS, LOOKS + 1, LOOKS + 2, LOOKS + 3]);
    assert!((ms(1400)..=ms(1600)).contains(&passed), "{passed:?}");
}

#[test]
fn glances_keep_coming_and_can_use_either_row() {
    let pet = pet(0, 0, 0, 0, 8);
    let mut rolls = [0, u64::MAX].into_iter().cycle();
    let mut life = Life::new(Box::new(move || rolls.next().unwrap()));
    let mut rows = std::collections::HashSet::new();
    for _ in 0..12_000 {
        life.advance(&pet, ms(25), true);
        rows.extend(shown(&life, &pet).map(|at| at >= LOOKS + 40));
    }
    assert_eq!(rows.len(), 2, "both rows of looks get used");
}

#[test]
fn a_row_shorter_than_a_glance_plays_all_it_has_and_still_lasts_about_as_long() {
    for frames in [1u8, 2, 3] {
        let pet = pet(0, 0, 0, 0, frames);
        let mut life = Life::new(fixed(0));
        life.advance(&pet, secs(6), true);
        assert_eq!(shown(&life, &pet), Some(LOOKS));
        let mut passed = Duration::ZERO;
        while life.sprite(&pet).is_some() {
            life.advance(&pet, ms(25), true);
            passed += ms(25);
            assert!(passed < secs(3), "{frames} frames never ended");
        }
        assert!((ms(1400)..=ms(1600)).contains(&passed), "{frames} frames: {passed:?}");
    }
}

#[test]
fn a_sheet_without_looks_skips_glances_and_stays_idle() {
    let pet = bare();
    let mut life = Life::new(fixed(0));
    assert!(!life.advance(&pet, secs(30), true));
    assert_eq!(shown(&life, &pet), None);
    assert_eq!(life.next_in(&pet), None);
}

// S2: stroll

#[test]
fn the_first_stroll_comes_between_1_and_4_minutes() {
    let pet = no_glances(8);
    let mut soon = Life::new(fixed(0));
    soon.advance(&pet, secs(59), true);
    assert_eq!(shown(&soon, &pet), None);
    soon.advance(&pet, secs(1), true);
    assert!(shown(&soon, &pet).is_some());

    let mut late = Life::new(fixed(u64::MAX));
    late.advance(&pet, secs(239), true);
    assert_eq!(shown(&late, &pet), None);
    late.advance(&pet, secs(1), true);
    assert!(shown(&late, &pet).is_some());
}

#[test]
fn glances_do_not_push_back_the_next_stroll() {
    let pet = full();
    let mut life = Life::new(fixed(u64::MAX));
    let mut passed = Duration::ZERO;
    let mut strolled = false;
    while passed < secs(241) && !strolled {
        life.advance(&pet, ms(100), true);
        passed += ms(100);
        strolled = (RIGHT..LEFT + 8).contains(&shown(&life, &pet).unwrap_or(0));
    }
    assert!(strolled, "still no stroll after {passed:?}");
}

/// Walks to the end, with a floor that lets every step through, and returns (pixels, seconds).
fn walk(life: &mut Life, pet: &Pet) -> (i32, f64) {
    let (mut travelled, mut passed) = (0, 0.0);
    while life.sprite(pet).is_some() {
        life.advance(pet, ms(50), true);
        passed += 0.05;
        if let Some(step) = life.step() {
            life.walked(step);
            travelled += step;
        }
        assert!(passed < 60.0, "the stroll never ended");
    }
    (travelled, passed)
}

#[test]
fn a_stroll_walks_60_to_200_pixels_at_about_40_per_second_and_stops() {
    let pet = no_glances(8);
    let (short, seconds) = walk(&mut into_a_stroll(&pet, 0), &pet);
    assert_eq!(short.abs(), 60);
    assert!((1.4..=1.6).contains(&seconds), "{seconds}");
    let (long, seconds) = walk(&mut into_a_stroll(&pet, u64::MAX), &pet);
    assert_eq!(long.abs(), 200);
    assert!((4.9..=5.1).contains(&seconds), "{seconds}");
}

#[test]
fn going_right_uses_the_right_row_and_left_the_left_row() {
    let pet = no_glances(8);
    // The low bit of the roll picks the way.
    let mut right = into_a_stroll(&pet, 0);
    assert!((RIGHT..RIGHT + 8).contains(&shown(&right, &pet).unwrap()));
    right.advance(&pet, ms(100), true);
    assert!(right.step().unwrap() > 0);
    let mut left = into_a_stroll(&pet, 1);
    assert!((LEFT..LEFT + 8).contains(&shown(&left, &pet).unwrap()));
    left.advance(&pet, ms(100), true);
    assert!(left.step().unwrap() < 0);
}

#[test]
fn at_the_edge_it_turns_around_and_finishes_the_distance_the_other_way() {
    let pet = no_glances(8);
    let mut life = into_a_stroll(&pet, 0);
    life.advance(&pet, ms(100), true);
    assert!(life.step().unwrap() > 0);
    life.walked(0);
    life.advance(&pet, ms(100), true);
    assert!((LEFT..LEFT + 8).contains(&shown(&life, &pet).unwrap()), "now the left row");
    assert!(life.step().unwrap() < 0);
    life.walked(-4);
    let (rest, _) = walk(&mut life, &pet);
    assert_eq!(rest, -56, "the whole 60 pixels get walked, away from the edge");
}

#[test]
fn a_step_cut_short_by_the_edge_counts_what_it_did_walk() {
    let pet = no_glances(8);
    let mut life = into_a_stroll(&pet, 0);
    life.advance(&pet, ms(100), true);
    assert_eq!(life.step(), Some(4));
    life.walked(1);
    let (rest, _) = walk(&mut life, &pet);
    assert_eq!(rest, -59);
}

#[test]
fn with_no_room_either_way_the_stroll_gives_up() {
    let pet = no_glances(8);
    let mut life = into_a_stroll(&pet, 0);
    for _ in 0..2 {
        life.advance(&pet, ms(100), true);
        life.step().unwrap();
        life.walked(0);
    }
    assert_eq!(shown(&life, &pet), None);
    assert_eq!(life.step(), None);
}

#[test]
fn a_long_stall_does_not_teleport_the_pet() {
    let pet = no_glances(8);
    let mut life = into_a_stroll(&pet, u64::MAX);
    life.advance(&pet, secs(30), true);
    assert!(life.step().unwrap().abs() <= 10);
}

#[test]
fn pressing_stops_a_stroll_and_a_drag_never_starts_one() {
    let pet = no_glances(8);
    let mut life = into_a_stroll(&pet, 0);
    life.pointer(&pet, Pointer::Pressed);
    assert_eq!(shown(&life, &pet), None);
    assert_eq!(life.step(), None);

    life.pointer(&pet, Pointer::Dragged(50));
    life.advance(&pet, secs(600), true);
    assert_eq!(shown(&life, &pet), None, "nothing starts while the pet is held");
    assert_eq!(life.step(), None);

    life.pointer(&pet, Pointer::Released);
    life.advance(&pet, secs(240), true);
    assert!((RIGHT..RIGHT + 8).contains(&shown(&life, &pet).unwrap()), "strolling comes back after the drag");
}

#[test]
fn a_rooted_pet_never_strolls_but_still_glances_and_dozes() {
    let pet = full();
    let mut life = Life::new(fixed(0)).rooted();
    let mut steps = 0;
    for _ in 0..200 {
        life.advance(&pet, ms(250), true);
        steps += life.step().iter().len();
        assert!(!(RIGHT..LEFT + 8).contains(&shown(&life, &pet).unwrap_or(0)));
    }
    assert_eq!(steps, 0);
    let mut life = Life::new(fixed(0)).rooted();
    assert!((LOOKS..LOOKS + 80).contains(&{ life.advance(&pet, secs(6), true); shown(&life, &pet).unwrap() }));
    dozing(&pet);
}

#[test]
fn a_sheet_without_running_rows_never_strolls() {
    let pet = pet(8, 4, 5, 0, 0);
    let mut life = Life::new(fixed(0));
    life.advance(&pet, secs(250), true);
    assert_eq!(shown(&life, &pet), None);
    assert_eq!(life.step(), None);
}

// S3: doze

#[test]
fn it_dozes_after_five_quiet_minutes_and_not_before() {
    let pet = pet(8, 0, 0, 0, 0);
    let mut life = Life::new(fixed(0));
    life.advance(&pet, ms(299_900), true);
    assert_eq!(shown(&life, &pet), None);
    life.advance(&pet, ms(100), true);
    assert_eq!(shown(&life, &pet), Some(DOZE));
}

#[test]
fn dozing_plays_once_up_to_the_hold_frame_and_stays_there_without_redrawing() {
    let pet = pet(8, 0, 0, 0, 0);
    let mut life = Life::new(fixed(0));
    life.advance(&pet, secs(300), true);
    let mut seen = vec![shown(&life, &pet).unwrap()];
    for _ in 0..40 {
        if life.advance(&pet, ms(100), true) {
            seen.extend(shown(&life, &pet));
        }
    }
    assert_eq!(seen, [DOZE, DOZE + 1, DOZE + 2, DOZE + 3, DOZE + 4]);
    assert!(!life.advance(&pet, secs(3600), true), "a held frame never asks to be drawn");
    assert_eq!(shown(&life, &pet), Some(DOZE + 4));
    assert!(life.next_in(&pet).unwrap() <= ms(250), "it keeps listening");
}

#[test]
fn the_hold_frame_comes_from_the_length_of_the_row_whatever_it_is() {
    for (frames, hold) in [(1u8, 0u8), (2, 1), (3, 1), (4, 2), (5, 2), (6, 3), (8, 4), (12, 6)] {
        let pet = pet(frames, 0, 0, 0, 0);
        let mut life = Life::new(fixed(0));
        life.advance(&pet, secs(300), true);
        life.advance(&pet, secs(60), true);
        assert_eq!(shown(&life, &pet), Some(DOZE + hold), "{frames} frames");
    }
}

#[test]
fn a_report_wakes_it_with_one_jump_and_then_it_continues() {
    let pet = full();
    let mut life = dozing(&pet);
    life.hear(&pet);
    assert_eq!(shown(&life, &pet), Some(JUMP));
    let mut seen = vec![JUMP];
    for _ in 0..12 {
        if life.advance(&pet, ms(125), true) {
            seen.extend(shown(&life, &pet));
        }
    }
    assert_eq!(seen, [JUMP, JUMP + 1, JUMP + 2, JUMP + 3, JUMP + 4]);
    assert_eq!(shown(&life, &pet), None);
    assert_eq!(life.step(), None);
}

#[test]
fn the_startled_jump_plays_even_when_the_report_brings_a_mood_and_then_gives_way() {
    let pet = full();
    let mut life = dozing(&pet);
    life.hear(&pet);
    life.advance(&pet, ms(125), false);
    assert_eq!(shown(&life, &pet), Some(JUMP + 1));
    life.advance(&pet, secs(2), false);
    assert_eq!(shown(&life, &pet), None, "the mood shows");
}

#[test]
fn a_press_wakes_it_too_and_the_click_does_not_also_wave() {
    let pet = full();
    let mut life = dozing(&pet);
    assert!(life.pointer(&pet, Pointer::Pressed));
    assert_eq!(shown(&life, &pet), Some(JUMP));
    life.pointer(&pet, Pointer::Released);
    life.advance(&pet, secs(2), true);
    assert_eq!(shown(&life, &pet), None);
}

#[test]
fn reports_and_presses_restart_the_five_minutes() {
    let pet = pet(8, 0, 0, 0, 0);
    let mut life = Life::new(fixed(0));
    life.advance(&pet, secs(299), true);
    life.hear(&pet);
    life.advance(&pet, secs(299), true);
    assert_eq!(shown(&life, &pet), None);
    life.pointer(&pet, Pointer::Pressed);
    life.pointer(&pet, Pointer::Released);
    life.advance(&pet, secs(299), true);
    assert_eq!(shown(&life, &pet), None);
    life.advance(&pet, secs(1), true);
    assert_eq!(shown(&life, &pet), Some(DOZE));
}

#[test]
fn a_sheet_without_the_row_stays_awake_and_without_the_jump_wakes_straight_to_idle() {
    let bare = bare();
    let mut life = Life::new(fixed(0));
    life.advance(&bare, secs(3600), true);
    assert_eq!(shown(&life, &bare), None);

    let no_jump = pet(8, 0, 0, 0, 0);
    let mut life = Life::new(fixed(0));
    life.advance(&no_jump, secs(300), true);
    life.hear(&no_jump);
    assert_eq!(shown(&life, &no_jump), None);
}

#[test]
fn it_never_dozes_while_a_mood_is_on() {
    let pet = full();
    let mut life = Life::new(fixed(0));
    life.advance(&pet, secs(3600), false);
    assert_eq!(shown(&life, &pet), None);
}

// S4: pointer

#[test]
fn entering_shows_a_head_turn_at_once() {
    let pet = full();
    let mut life = Life::new(fixed(0));
    life.advance(&pet, ms(1), true);
    assert!(life.pointer(&pet, Pointer::Entered));
    assert_eq!(shown(&life, &pet), Some(LOOKS));
}

#[test]
fn entering_does_nothing_without_looks_or_on_a_sleeping_pet_or_during_a_mood() {
    let v1 = v1();
    let mut life = Life::new(fixed(0));
    life.advance(&v1, ms(1), true);
    assert!(!life.pointer(&v1, Pointer::Entered));
    assert_eq!(shown(&life, &v1), None);

    let pet = full();
    let mut asleep = dozing(&pet);
    assert!(!asleep.pointer(&pet, Pointer::Entered));
    assert_eq!(shown(&asleep, &pet), Some(DOZE + 4));

    let mut busy = Life::new(fixed(0));
    busy.advance(&pet, ms(1), false);
    assert!(!busy.pointer(&pet, Pointer::Entered));
}

#[test]
fn a_click_waves_once_and_a_drag_does_not() {
    let pet = full();
    let mut life = Life::new(fixed(0));
    life.advance(&pet, ms(1), true);
    life.pointer(&pet, Pointer::Pressed);
    assert_eq!(shown(&life, &pet), None, "nothing until the release");
    assert!(life.pointer(&pet, Pointer::Released));
    assert_eq!(shown(&life, &pet), Some(WAVE));
    let mut seen = vec![WAVE];
    for _ in 0..20 {
        if life.advance(&pet, ms(100), true) {
            seen.extend(shown(&life, &pet));
        }
    }
    assert_eq!(seen, [WAVE, WAVE + 1, WAVE + 2, WAVE + 3]);
    assert_eq!(shown(&life, &pet), None);

    let mut dragged = Life::new(fixed(0));
    dragged.advance(&pet, ms(1), true);
    dragged.pointer(&pet, Pointer::Pressed);
    dragged.pointer(&pet, Pointer::Dragged(4));
    assert!(!dragged.pointer(&pet, Pointer::Released));
    assert_eq!(shown(&dragged, &pet), None);
}

#[test]
fn a_hand_that_shakes_a_little_still_counts_as_a_click() {
    let pet = full();
    let mut life = Life::new(fixed(0));
    life.advance(&pet, ms(1), true);
    life.pointer(&pet, Pointer::Pressed);
    life.pointer(&pet, Pointer::Dragged(1));
    life.pointer(&pet, Pointer::Dragged(2));
    life.pointer(&pet, Pointer::Released);
    assert_eq!(shown(&life, &pet), Some(WAVE));
}

#[test]
fn losing_the_grab_is_not_a_click() {
    let pet = full();
    let mut life = Life::new(fixed(0));
    life.advance(&pet, ms(1), true);
    life.pointer(&pet, Pointer::Pressed);
    life.pointer(&pet, Pointer::Left);
    assert!(!life.pointer(&pet, Pointer::Released));
    assert_eq!(shown(&life, &pet), None);
}

#[test]
fn a_click_during_a_mood_does_not_wave() {
    let pet = full();
    let mut life = Life::new(fixed(0));
    life.advance(&pet, ms(1), false);
    life.pointer(&pet, Pointer::Pressed);
    assert!(!life.pointer(&pet, Pointer::Released));
    assert_eq!(shown(&life, &pet), None);
}

#[test]
fn a_sheet_without_the_wave_row_ignores_the_click() {
    let pet = pet(8, 0, 5, 8, 8);
    let mut life = Life::new(fixed(0));
    life.advance(&pet, ms(1), true);
    life.pointer(&pet, Pointer::Pressed);
    assert!(!life.pointer(&pet, Pointer::Released));
}

// Agent moods and reports always win.

#[test]
fn any_report_interrupts_a_glance_a_stroll_or_a_wave_at_once() {
    let pet = full();
    let mut glancing = Life::new(fixed(0));
    glancing.advance(&pet, secs(6), true);
    assert!(shown(&glancing, &pet).is_some());
    glancing.hear(&pet);
    assert_eq!(shown(&glancing, &pet), None);

    let no_looks = no_glances(8);
    let mut strolling = into_a_stroll(&no_looks, 0);
    strolling.hear(&no_looks);
    assert_eq!(shown(&strolling, &no_looks), None);
    assert_eq!(strolling.step(), None);

    let mut waving = Life::new(fixed(0));
    waving.advance(&pet, ms(1), true);
    waving.pointer(&pet, Pointer::Pressed);
    waving.pointer(&pet, Pointer::Released);
    waving.hear(&pet);
    assert_eq!(shown(&waving, &pet), None);
}

#[test]
fn a_mood_ends_any_act_and_stops_new_ones_until_idle_returns() {
    let pet = full();
    let mut life = Life::new(fixed(0));
    life.advance(&pet, secs(6), true);
    assert!(shown(&life, &pet).is_some());
    assert!(life.advance(&pet, Duration::ZERO, false), "the end of the act must be drawn");
    assert_eq!(shown(&life, &pet), None);
    life.advance(&pet, secs(250), false);
    assert_eq!(shown(&life, &pet), None);
    assert_eq!(life.step(), None);
    life.advance(&pet, secs(6), true);
    assert!(shown(&life, &pet).is_some(), "life goes on in idle");
}

#[test]
fn the_end_of_a_stroll_is_due_at_once_so_a_running_frame_does_not_linger() {
    let pet = no_glances(8);
    let mut life = into_a_stroll(&pet, 0);
    let mut left = 60;
    while left > 0 {
        life.advance(&pet, ms(50), true);
        if let Some(step) = life.step() {
            life.walked(step);
            left -= step;
        }
    }
    assert_eq!(life.next_in(&pet), Some(Duration::ZERO));
    assert!(life.advance(&pet, Duration::ZERO, true));
    assert_eq!(life.next_in(&pet), None);
}

// S6: a v1 standard sheet gets everything its rows allow.

#[test]
fn a_v1_sheet_strolls_dozes_and_waves_but_has_no_head_turns() {
    let pet = v1();
    let mut life = into_a_stroll(&pet, 0);
    assert!((RIGHT..RIGHT + 8).contains(&shown(&life, &pet).unwrap()));
    walk(&mut life, &pet);
    life.pointer(&pet, Pointer::Pressed);
    life.pointer(&pet, Pointer::Released);
    assert_eq!(shown(&life, &pet), Some(WAVE));
    dozing(&pet);
}
