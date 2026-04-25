mod cli;
mod directory;
mod markdown;
mod pdf;
mod readme;

/// Program entry point that delegates execution to the CLI runner.
///
/// # Examples
///
/// ```
/// // Start the application by invoking the entry point.
/// crate::main();
/// ```
fn main() {
    cli::run();
}
