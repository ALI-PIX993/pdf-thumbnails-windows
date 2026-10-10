//! Windows shell glue is kept separate from the safe PDF rendering API.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(windows)]
#[allow(unsafe_code, non_snake_case)]
mod windows;
