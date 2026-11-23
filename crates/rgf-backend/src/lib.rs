//! Ground-station backend.
//!
//! [`Router`] is the logic: it passes messages between UIs and the vehicle, retries
//! anything that needs an acknowledgement, and runs the RTK base station. [`server`]
//! puts it behind a UDP vehicle link and a WebSocket for UIs.

pub mod base;
pub mod router;
pub mod server;

pub use router::Router;
