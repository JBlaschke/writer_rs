//! `init` command entry point; implemented by T72.

use std::process::ExitCode;

use super::{GlobalArgs, InitArgs};

/// Runs `writer init` (the T72 milestone replaces this body).
pub fn run(_: &InitArgs, _: &GlobalArgs) -> anyhow::Result<ExitCode> {
    Err(crate::pipeline::not_implemented("init", "T72").into())
}
