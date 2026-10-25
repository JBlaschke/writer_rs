//! Browser PDF output and browser test support (cli.md “build”; PLAN.md §3.2, §7).
//! Owner: T00 (module list); T77 and T25 (children). Stub created by T00.

pub mod chrome;
#[cfg(feature = "browser-tests")]
pub mod devtools;
