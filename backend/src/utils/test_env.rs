//! Environment changes for unit tests.
//!
//! `set_var` and `remove_var` are `unsafe` since edition 2024 because another
//! thread may read the environment through libc (`getenv`) while it changes.
//! The app reads its settings through `std::env`, which serialises with these
//! calls. What is left is a libc read on another test thread, a DNS lookup for
//! instance. The tests accept that, as they did before the edition change.

use std::ffi::OsStr;

pub(crate) fn set_env<K: AsRef<OsStr>, V: AsRef<OsStr>>(key: K, value: V) {
    // SAFETY: test-only; see the module comment.
    unsafe { std::env::set_var(key, value) };
}

pub(crate) fn remove_env<K: AsRef<OsStr>>(key: K) {
    // SAFETY: test-only; see the module comment.
    unsafe { std::env::remove_var(key) };
}
