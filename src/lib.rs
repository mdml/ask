//! Pre-alpha placeholder library surface for `ask`.

/// Returns the placeholder message printed by the binary.
pub fn placeholder_message() -> &'static str {
    "ask: pre-alpha placeholder"
}

/// Prints the placeholder message to stdout.
pub fn run() {
    println!("{}", placeholder_message());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_message_is_non_empty() {
        assert!(!placeholder_message().is_empty());
    }

    #[test]
    fn placeholder_message_contains_ask() {
        assert!(placeholder_message().contains("ask"));
    }

    #[test]
    fn run_does_not_panic() {
        run();
    }
}
