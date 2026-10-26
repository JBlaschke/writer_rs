//! `serve` command entry point; implemented by T23.

use std::process::ExitCode;

use super::{GlobalArgs, ServeArgs};

/// Runs `writer serve` (the T23 milestone replaces this body).
pub fn run(_: &ServeArgs, _: &GlobalArgs) -> anyhow::Result<ExitCode> {
    Err(crate::pipeline::not_implemented("serve", "T23").into())
}
