use std::process::ExitCode;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    ask::run(std::env::args().skip(1)).await
}
