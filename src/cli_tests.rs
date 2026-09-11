use super::*;

fn words(values: &[&str]) -> Vec<String> {
    values.iter().map(ToString::to_string).collect()
}

fn piped(values: &[&str]) -> Result<Command, String> {
    parse(words(values), false)
}

fn terminal(values: &[&str]) -> Result<Command, String> {
    parse(words(values), true)
}

fn query(text: &str) -> Result<Command, String> {
    Ok(Command::Query(Mode::New, Some(text.to_string())))
}

fn usage() -> Result<Command, String> {
    Err(USAGE.to_string())
}

#[test]
fn direct_prompt_joins_words() {
    assert_eq!(piped(&["how", "now"]), query("how now"));
}

#[test]
fn aliases_strip_the_command() {
    assert_eq!(piped(&["new", "hello"]), query("hello"));
    assert_eq!(piped(&["n", "hello"]), query("hello"));
}

#[test]
fn empty_query_words_are_resolved_from_stdin() {
    for args in [&[][..], &["new"][..], &["n"][..]] {
        assert_eq!(piped(args), Ok(Command::Query(Mode::New, None)));
        assert_eq!(terminal(args), Ok(Command::Query(Mode::New, None)));
    }
}

#[test]
fn reply_and_its_alias_continue_the_current_thread() {
    for name in ["reply", "r"] {
        assert_eq!(
            piped(&[name, "and", "then"]),
            Ok(Command::Query(Mode::Reply, Some("and then".to_string())))
        );
        assert_eq!(terminal(&[name]), Ok(Command::Query(Mode::Reply, None)));
    }
}

#[test]
fn init_and_its_alias_take_no_arguments() {
    assert_eq!(terminal(&["init"]), Ok(Command::Init));
    assert_eq!(terminal(&["i"]), Ok(Command::Init));
    assert_eq!(terminal(&["init", "x"]), usage());
    assert_eq!(terminal(&["i", "x"]), usage());
}

#[test]
fn configure_requires_a_known_verb() {
    assert_eq!(piped(&["configure"]), usage());
    assert_eq!(piped(&["c"]), usage());
    assert_eq!(piped(&["configure", "edit"]), usage());
    assert_eq!(piped(&["c", "check", "one", "two"]), usage());
}

#[test]
fn configure_verbs_and_aliases_read_a_named_file() {
    let file = Source::File(PathBuf::from("candidate.toml"));
    assert_eq!(
        terminal(&["configure", "check", "candidate.toml"]),
        Ok(Command::Configure(Action::Check(file)))
    );
    let file = Source::File(PathBuf::from("candidate.toml"));
    assert_eq!(
        terminal(&["c", "apply", "candidate.toml"]),
        Ok(Command::Configure(Action::Apply(file)))
    );
}

#[test]
fn a_dash_and_an_omitted_argument_both_mean_standard_input() {
    assert_eq!(
        terminal(&["configure", "check", "-"]),
        Ok(Command::Configure(Action::Check(Source::Stdin)))
    );
    assert_eq!(
        piped(&["configure", "check"]),
        Ok(Command::Configure(Action::Check(Source::Stdin)))
    );
    assert_eq!(
        piped(&["c", "apply"]),
        Ok(Command::Configure(Action::Apply(Source::Stdin)))
    );
}

#[test]
fn omitting_the_argument_at_a_terminal_is_a_usage_error() {
    let message = terminal(&["configure", "apply"]).unwrap_err();
    assert!(
        message.starts_with("reading a configuration from a terminal is not supported"),
        "{message}"
    );
    assert!(message.contains(USAGE), "{message}");
}

#[test]
fn sources_describe_themselves() {
    assert_eq!(Source::Stdin.to_string(), "standard input");
    assert_eq!(
        Source::File(PathBuf::from("a/b.toml")).to_string(),
        "'a/b.toml'"
    );
}
