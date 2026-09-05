const USAGE: &str = "usage: ask [new|n] <prompt words...>";

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<String, String> {
    let mut words: Vec<String> = args.into_iter().collect();
    if matches!(words.first().map(String::as_str), Some("new" | "n")) {
        words.remove(0);
    }
    if words.is_empty() {
        return Err(USAGE.to_string());
    }
    Ok(words.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(values: &[&str]) -> Vec<String> {
        values.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn direct_prompt_joins_words() {
        assert_eq!(parse(words(&["how", "now"])), Ok("how now".to_string()));
    }

    #[test]
    fn aliases_strip_the_command() {
        assert_eq!(parse(words(&["new", "hello"])), Ok("hello".to_string()));
        assert_eq!(parse(words(&["n", "hello"])), Ok("hello".to_string()));
    }

    #[test]
    fn empty_prompt_is_a_usage_error() {
        assert_eq!(parse(words(&[])), Err(USAGE.to_string()));
        assert_eq!(parse(words(&["new"])), Err(USAGE.to_string()));
    }
}
