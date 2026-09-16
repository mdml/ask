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
    Ok(Command::Query(
        Mode::New,
        QueryOptions {
            profile: None,
            words: Some(text.to_string()),
        },
    ))
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
        assert_eq!(
            piped(args),
            Ok(Command::Query(
                Mode::New,
                QueryOptions {
                    profile: None,
                    words: None
                }
            ))
        );
        assert_eq!(
            terminal(args),
            Ok(Command::Query(
                Mode::New,
                QueryOptions {
                    profile: None,
                    words: None
                }
            ))
        );
    }
}

#[test]
fn reply_and_its_alias_continue_the_current_thread() {
    for name in ["reply", "r"] {
        assert_eq!(
            piped(&[name, "and", "then"]),
            Ok(Command::Query(
                Mode::Reply,
                QueryOptions {
                    profile: None,
                    words: Some("and then".to_string())
                }
            ))
        );
        assert_eq!(
            terminal(&[name]),
            Ok(Command::Query(
                Mode::Reply,
                QueryOptions {
                    profile: None,
                    words: None
                }
            ))
        );
    }
}

#[test]
fn profile_overrides_apply_only_to_new_queries() {
    assert_eq!(
        piped(&["--profile", "terse", "hello"]),
        Ok(Command::Query(
            Mode::New,
            QueryOptions {
                profile: Some("terse".to_string()),
                words: Some("hello".to_string())
            }
        ))
    );
    assert_eq!(
        piped(&["new", "-p", "terse", "hello"]),
        Ok(Command::Query(
            Mode::New,
            QueryOptions {
                profile: Some("terse".to_string()),
                words: Some("hello".to_string())
            }
        ))
    );
    let reply_profile = piped(&["reply", "--profile", "terse", "hello"]).unwrap_err();
    assert!(reply_profile.starts_with(
        "--profile and -p apply only to new queries; replies use the profile captured when their thread was created"
    ));
    assert!(reply_profile.contains(USAGE));
    let missing_profile = piped(&["--profile"]).unwrap_err();
    assert!(missing_profile.starts_with("--profile requires a profile name"));
    assert!(missing_profile.contains(USAGE));
}

#[test]
fn init_and_its_alias_take_no_arguments() {
    assert_eq!(terminal(&["init"]), Ok(Command::Init));
    assert_eq!(terminal(&["i"]), Ok(Command::Init));
    assert_eq!(terminal(&["init", "x"]), usage());
    assert_eq!(terminal(&["i", "x"]), usage());
}

#[test]
fn help_and_version_take_no_arguments() {
    for name in ["help", "--help", "-h"] {
        assert_eq!(terminal(&[name]), Ok(Command::Help));
        assert_eq!(piped(&[name, "x"]), usage());
    }
    for name in ["version", "--version", "-V"] {
        assert_eq!(terminal(&[name]), Ok(Command::Version));
        assert_eq!(piped(&[name, "x"]), usage());
    }
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

#[test]
fn recall_commands_and_aliases_take_no_arguments() {
    for (name, expected) in [
        ("thread", Command::Thread),
        ("t", Command::Thread),
        ("stats", Command::Stats),
    ] {
        assert_eq!(terminal(&[name]), Ok(expected));
        assert_eq!(piped(&[name, "x"]), usage());
    }
}

#[test]
fn doctor_accepts_live_and_all_flags() {
    assert_eq!(
        terminal(&["doctor"]),
        Ok(Command::Doctor {
            live: false,
            all: false
        })
    );
    assert_eq!(
        terminal(&["d", "--live"]),
        Ok(Command::Doctor {
            live: true,
            all: false
        })
    );
    assert_eq!(
        terminal(&["doctor", "--live", "--all"]),
        Ok(Command::Doctor {
            live: true,
            all: true
        })
    );
    let message = terminal(&["doctor", "--all"]).unwrap_err();
    assert!(message.contains("--all requires --live"), "{message}");
    assert_eq!(terminal(&["doctor", "--live", "x"]), usage());
}

#[test]
fn switch_takes_an_optional_positive_thread_id() {
    for name in ["switch", "s"] {
        assert_eq!(terminal(&[name]), Ok(Command::Switch(None)));
        assert_eq!(piped(&[name, "12"]), Ok(Command::Switch(Some(12))));
        for bad in ["0", "-1", "+3", "x", "99999999999999999999"] {
            assert_eq!(piped(&[name, bad]), usage(), "{bad}");
        }
        assert_eq!(piped(&[name, "1", "2"]), usage());
    }
}

#[test]
fn other_words_remain_prompts() {
    assert_eq!(piped(&["statistics"]), query("statistics"));
    assert_eq!(piped(&["threads", "please"]), query("threads please"));
}

#[test]
fn query_command_words_after_the_subcommand_remain_prompt_text() {
    assert_eq!(
        piped(&["new", "reply", "to", "this"]),
        query("reply to this")
    );
    assert_eq!(
        piped(&["reply", "new", "question"]),
        Ok(Command::Query(
            Mode::Reply,
            QueryOptions {
                profile: None,
                words: Some("new question".to_string())
            }
        ))
    );
}
