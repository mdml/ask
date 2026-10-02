use super::*;

fn run(chunks: &[&str]) -> String {
    let mut scanner = InlineReasoning::new();
    let mut answer: String = chunks.iter().map(|chunk| scanner.push(chunk)).collect();
    answer.push_str(&scanner.finish());
    answer
}

/// Every way to cut `text` into two chunks, and into one-character chunks.
fn splits(text: &str) -> Vec<Vec<String>> {
    let mut all: Vec<Vec<String>> = (0..=text.len())
        .filter(|&at| text.is_char_boundary(at))
        .map(|at| vec![text[..at].to_string(), text[at..].to_string()])
        .collect();
    all.push(text.chars().map(String::from).collect());
    all
}

fn assert_every_split(text: &str, expected: &str) {
    for chunks in splits(text) {
        let parts: Vec<&str> = chunks.iter().map(String::as_str).collect();
        assert_eq!(run(&parts), expected, "chunks {parts:?}");
    }
}

#[test]
fn a_leading_block_is_removed_at_every_split() {
    assert_every_split("<think>plan</think>\n\nAnswer", "Answer");
}

#[test]
fn leading_whitespace_before_the_tag_is_removed() {
    for lead in [" ", "\n", "\r\n\t ", "\u{c}"] {
        assert_every_split(&format!("{lead}<think>x</think>Answer"), "Answer");
    }
}

#[test]
fn tag_names_match_ignoring_ascii_case() {
    assert_every_split("<THINK>x</Think>\nAnswer", "Answer");
    assert_every_split("<tHiNk>x</tHINK>Answer", "Answer");
}

#[test]
fn only_whitespace_after_the_closing_tag_is_removed() {
    assert_every_split("<think>x</think> \t\r\n\n  Answer \n", "Answer \n");
    assert_every_split("<think>x</think>", "");
    assert_every_split("<think>x</think>\n\n", "");
}

#[test]
fn text_after_the_block_is_unchanged() {
    assert_every_split(
        "<think>x</think>a  b\n\n<think>y</think>\n",
        "a  b\n\n<think>y</think>\n",
    );
}

#[test]
fn answers_without_the_tag_pass_through_unchanged() {
    for text in [
        "",
        "Hello",
        "  Hello\n",
        "<thin>",
        "<thin and more text",
        "<thinking>deeply</thinking>",
        "<think",
        "<thinkx>",
        "< think>",
        "</think>",
        "Use <think> tags\n",
        "\n\n<b>bold</b>",
        "é<think>x</think>",
    ] {
        assert_every_split(text, text);
    }
}

#[test]
fn a_tag_that_is_not_at_the_start_is_not_a_block() {
    assert_every_split(
        "Answer <think>x</think> more",
        "Answer <think>x</think> more",
    );
    assert_every_split("x<think>y</think>", "x<think>y</think>");
}

#[test]
fn a_closing_tag_split_across_chunks_ends_the_block() {
    assert_eq!(run(&["<think>a<", "/thi", "nk", ">", " ok"]), "ok");
    assert_eq!(run(&["<think>a</", "think", ">\n", "\nok"]), "ok");
}

#[test]
fn near_miss_closing_text_inside_the_block_stays_inside() {
    assert_every_split("<think></thin></thinking></think>Answer", "Answer");
    assert_every_split("<think><</think>A", "A");
}

#[test]
fn the_first_closing_tag_ends_the_block_even_with_nested_openings() {
    assert_every_split("<think>a<think>b</think>c</think>d", "c</think>d");
    assert_every_split("<think><think></think></think>", "</think>");
}

#[test]
fn multi_byte_text_around_the_tags_is_preserved() {
    assert_every_split("<think>é日本</think>\n日本語 é", "日本語 é");
    assert_every_split("<think>\u{1f600}<</think>\u{1f600}", "\u{1f600}");
    assert_every_split("\u{a0}<think>x</think>y", "\u{a0}<think>x</think>y");
}

#[test]
fn empty_chunks_change_nothing() {
    assert_eq!(
        run(&["", "<th", "", "ink>", "", "x</think>", "", "A", ""]),
        "A"
    );
    assert_eq!(run(&["", ""]), "");
}

#[test]
fn an_unclosed_block_yields_an_empty_answer() {
    assert_every_split("<think>never closed", "");
    assert_every_split("<think>almost</thin", "");
    assert_every_split("  <think>", "");
}

#[test]
fn withheld_tag_prefixes_are_flushed_at_the_end() {
    assert_eq!(run(&["<thi"]), "<thi");
    assert_eq!(run(&[" \n<", "thin"]), " \n<thin");
    assert_eq!(run(&["  "]), "  ");
}

#[test]
fn text_is_released_as_soon_as_the_tag_is_ruled_out() {
    let mut scanner = InlineReasoning::new();
    assert_eq!(scanner.push("  "), "");
    assert_eq!(scanner.push("<th"), "");
    assert_eq!(scanner.push("ought"), "  <thought");
    assert_eq!(scanner.push("<think>"), "<think>");
    assert_eq!(scanner.finish(), "");
}

#[test]
fn nothing_is_emitted_inside_the_block_and_streaming_resumes_after_it() {
    let mut scanner = InlineReasoning::new();
    assert_eq!(scanner.push("<think>"), "");
    assert_eq!(scanner.push("long reasoning</thi"), "");
    assert_eq!(scanner.push("nk>\n"), "");
    assert_eq!(scanner.push("\nfirst"), "first");
    assert_eq!(scanner.push(" second"), " second");
    assert_eq!(scanner.finish(), "");
}
