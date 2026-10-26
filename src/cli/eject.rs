//! `eject` command entry point; implemented by T72.

use std::process::ExitCode;

use super::{EjectArgs, GlobalArgs};

/// Runs `writer eject` (the T72 milestone replaces this body).
pub fn run(_: &EjectArgs, _: &GlobalArgs) -> anyhow::Result<ExitCode> {
    Err(crate::pipeline::not_implemented("eject", "T72").into())
}
