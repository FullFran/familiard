//! The words a pet with a label shows above its head. Measuring text is someone else's
//! job, so whoever knows a font passes that in.
use crate::Mood;

/// Far more than fits on a caption; keeps a pasted novel from being measured.
const MAX_LABEL_CHARS: usize = 256;
const ELLIPSIS: char = '\u{2026}';

/// A mood per distinct caption, to reserve room for the widest of them up front.
pub fn caption_moods() -> [Mood; 4] {
    [Mood::Idle, Mood::Working, Mood::Waiting, Mood::Done]
}

/// The mood in [`caption_moods`] that reads the same as this one.
pub fn caption_mood(mood: Mood) -> Mood {
    match mood {
        Mood::Working | Mood::Waiting | Mood::Done => mood,
        Mood::Celebrating => Mood::Done,
        Mood::Idle | Mood::Failed | Mood::Reviewing => Mood::Idle,
    }
}

fn note(mood: Mood) -> &'static str {
    match caption_mood(mood) {
        Mood::Working => " \u{b7} working",
        Mood::Waiting => " \u{b7} needs you",
        Mood::Done => " \u{b7} done",
        _ => "",
    }
}

/// A label fit to print on one line: no control characters, no surrounding space.
pub fn clean_label(label: &str) -> String {
    let printable: String = label.chars().filter(|c| !c.is_control()).take(MAX_LABEL_CHARS).collect();
    printable.trim().to_string()
}

/// The caption for a mood, shortening the label with an ellipsis until `width_of` says it
/// fits in `max_width`. The state is what matters, so it is never cut.
pub fn fit_caption(label: &str, mood: Mood, max_width: u32, width_of: impl Fn(&str) -> u32) -> String {
    let note = note(mood);
    let whole = format!("{label}{note}");
    if width_of(&whole) <= max_width {
        return whole;
    }
    let shortened = |kept: usize| {
        let start: String = label.chars().take(kept).collect();
        format!("{}{ELLIPSIS}{note}", start.trim_end())
    };
    let (mut fits, mut too_long) = (0, label.chars().count());
    while too_long - fits > 1 {
        let middle = (fits + too_long) / 2;
        if width_of(&shortened(middle)) <= max_width {
            fits = middle;
        } else {
            too_long = middle;
        }
    }
    shortened(fits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str) -> u32 {
        text.chars().count() as u32
    }

    #[test]
    fn says_the_state_after_the_label() {
        let text = |mood| fit_caption("Bug", mood, 100, chars);
        assert_eq!(text(Mood::Idle), "Bug");
        assert_eq!(text(Mood::Working), "Bug \u{b7} working");
        assert_eq!(text(Mood::Waiting), "Bug \u{b7} needs you");
        assert_eq!(text(Mood::Done), "Bug \u{b7} done");
        assert_eq!(text(Mood::Celebrating), "Bug \u{b7} done");
    }

    #[test]
    fn shortens_the_label_but_never_the_state() {
        let text = fit_caption("abcdefghij", Mood::Working, 14, chars);
        assert_eq!(text, "abc\u{2026} \u{b7} working");
        assert_eq!(fit_caption("abcdefghij", Mood::Idle, 10, chars), "abcdefghij");
        assert_eq!(fit_caption("abcdefghij", Mood::Idle, 9, chars), "abcdefgh\u{2026}");
    }

    #[test]
    fn a_cut_does_not_leave_a_space_before_the_ellipsis_or_split_a_character() {
        assert_eq!(fit_caption("ab cdefgh", Mood::Idle, 4, chars), "ab\u{2026}");
        assert_eq!(fit_caption("\u{f1}\u{f1}\u{f1}\u{f1}", Mood::Idle, 3, chars), "\u{f1}\u{f1}\u{2026}");
    }

    #[test]
    fn a_label_loses_control_characters_and_outer_space() {
        assert_eq!(clean_label("  a\n\x1b[2Jb \t"), "a[2Jb");
        assert_eq!(clean_label(&"x".repeat(1000)).len(), MAX_LABEL_CHARS);
    }
}
