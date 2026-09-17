use super::*;
use crate::store::StoredTurn;

fn turn(prompt: &str, answer: &str, reason: Option<&str>) -> StoredTurn {
    StoredTurn {
        prompt: prompt.to_string(),
        answer: answer.to_string(),
        reason: reason.map(ToString::to_string),
    }
}

#[test]
fn threads_render_speaker_labels_content_and_incomplete_markers() {
    let view = ThreadView {
        id: 7,
        profile: "terse".to_string(),
        model: "fake-model".to_string(),
        turns: vec![
            turn("first\n\nsecond line\n", "**4**\n\n", None),
            turn("empty", "", None),
            turn("cut", "part", Some("provider request failed: reset")),
            turn("closed", "", Some("output closed")),
        ],
    };
    assert_eq!(
        render(&view),
        "thread 7 · profile terse · model fake-model\n\n\
         You:\nfirst\n\nsecond line\n\nAssistant:\n**4**\n\n\
         You:\nempty\n\nAssistant:\n\n\
         You:\ncut\n\nAssistant:\npart\n[incomplete: provider request failed: reset]\n\n\
         You:\nclosed\n\nAssistant:\n[incomplete: output closed]\n"
    );
}

#[test]
fn menu_entries_are_single_safe_lines() {
    let long = "x".repeat(OPENING_CHARS + 1);
    let threads = [
        ThreadSummary {
            id: 3,
            updated_at_ms: 60_000,
            profile: "terse".to_string(),
            model: "m".to_string(),
            turns: 2,
            opening: "  tab\there\u{1b}[31m\nsecond".to_string(),
            current: true,
        },
        ThreadSummary {
            id: 1,
            updated_at_ms: 0,
            profile: "p\u{1b}]0;title\u{7}".to_string(),
            model: "m\u{9b}2J\r".to_string(),
            turns: 1,
            opening: long,
            current: false,
        },
    ];
    let expected = format!(
        " 1. thread 3 (current) · 1970-01-01 00:01 UTC · 2 turns · terse · m · tab here [31m\n \
         2. thread 1 · 1970-01-01 00:00 UTC · 1 turn · p ]0;title  · m 2J  · {}…\n\
         select a thread [1-2]: ",
        "x".repeat(OPENING_CHARS)
    );
    assert_eq!(menu(&threads), expected);
}

#[test]
fn menu_entries_replace_invisible_format_characters_but_keep_joiners() {
    let threads = [ThreadSummary {
        id: 1,
        updated_at_ms: 0,
        profile: "p\u{202e}q".to_string(),
        model: "m\u{200b}\u{feff}\u{e0041}".to_string(),
        turns: 1,
        opening: "\u{2066}hi\u{2069}\u{ad}x 👨\u{200d}👩 نمی\u{200c}خواهم".to_string(),
        current: false,
    }];
    assert_eq!(
        menu(&threads),
        " 1. thread 1 · 1970-01-01 00:00 UTC · 1 turn · p q · m    ·  hi  x 👨\u{200d}👩 نمی\u{200c}خواهم\n\
         select a thread [1-1]: "
    );
}

#[test]
fn terminal_menu_keeps_the_prompt_preview_before_long_metadata() {
    let thread = ThreadSummary {
        id: 1,
        updated_at_ms: 0,
        profile: "long-profile-name".repeat(4),
        model: "long-model-name".repeat(4),
        turns: 2,
        opening: "recognizable question".to_string(),
        current: true,
    };
    let visible: String = terminal_entry(&thread).chars().take(70).collect();
    assert!(visible.contains("recognizable question"), "{visible}");
    assert!(visible.contains("2 turns"), "{visible}");
}
