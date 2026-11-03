//! The compositor: event loop, clients and input. Policy lives in the crates below
//! this one; this is where it meets luminaria.

pub mod app;
pub mod devices;
pub mod grab;
pub mod ime;
pub mod input;
pub mod ipc;
pub mod lock;
pub mod outputs;
pub mod state;
pub mod touch;
pub mod watch;
pub mod windows;
pub mod workspaces;
pub mod xwayland;
