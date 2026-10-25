//! Command-line parsing and command dispatch (cli.md).
//! Owner: T16. Stub created by T00.

pub mod build;
pub mod check;
pub mod dump;
pub mod eject;
pub mod init;
pub mod report;
pub mod serve;

/// Runs the command-line interface and returns its process exit status.
///
/// T16 replaces this scaffold with command parsing and dispatch.
pub fn run() -> std::process::ExitCode {
    eprintln!("writer: not implemented yet (T16)");
    std::process::ExitCode::from(2)
}
