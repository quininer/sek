// # Safety
//
// We don’t use `spawn`, so there is no task that runs at the same time.
#![allow(clippy::await_holding_refcell_ref)]

#[macro_use]
pub mod util;
pub mod ui;
pub mod shell;
pub mod editor;
pub mod config;
pub mod cache;
pub mod ipc;
