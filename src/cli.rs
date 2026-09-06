const USAGE: &str = "usage: ask [new|n] <prompt words...> | ask [configure|c]";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Query(String),
    Configure,
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut words: Vec<String> = args.into_iter().collect();
    match words.first().map(String::as_str) {
        Some("configure" | "c") if words.len() == 1 => return Ok(Command::Configure),
        Some("configure" | "c") => return Err(USAGE.to_string()),
        Some("new" | "n") => {
            words.remove(0);
        }
        _ => {}
    }
    if words.is_empty() {
        return Err(USAGE.to_string());
    }
    Ok(Command::Query(words.join(" ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(values: &[&str]) -> Vec<String> {
        values.iter().map(ToString::to_string).collect()
    }

    fn query(text: &str) -> Result<Command, String> {
        Ok(Command::Query(text.to_string()))
    }

    #[test]
    fn direct_prompt_joins_words() {
        assert_eq!(parse(words(&["how", "now"])), query("how now"));
    }

    #[test]
    fn aliases_strip_the_command() {
        assert_eq!(parse(words(&["new", "hello"])), query("hello"));
        assert_eq!(parse(words(&["n", "hello"])), query("hello"));
    }

    #[test]
    fn empty_prompt_is_a_usage_error() {
        assert_eq!(parse(words(&[])), Err(USAGE.to_string()));
        assert_eq!(parse(words(&["new"])), Err(USAGE.to_string()));
    }

    #[test]
    fn configure_and_its_alias_take_no_arguments() {
        assert_eq!(parse(words(&["configure"])), Ok(Command::Configure));
        assert_eq!(parse(words(&["c"])), Ok(Command::Configure));
        assert_eq!(parse(words(&["configure", "x"])), Err(USAGE.to_string()));
        assert_eq!(parse(words(&["c", "x"])), Err(USAGE.to_string()));
    }
}
