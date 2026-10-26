//! `check` command entry point; implemented by T1C.

use std::process::ExitCode;

use super::{CheckArgs, GlobalArgs};

/// Runs `writer check` (the T1C milestone replaces this body).
pub fn run(_: &CheckArgs, _: &GlobalArgs) -> anyhow::Result<ExitCode> {
    Err(crate::pipeline::not_implemented("check", "T1C").into())
}
