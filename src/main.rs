fn main() {
    ask::run();
}

#[cfg(test)]
mod tests {
    #[test]
    fn main_does_not_panic() {
        super::main();
    }
}
