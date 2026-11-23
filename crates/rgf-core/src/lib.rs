//! Shared, IO-free logic for the robotics GUI framework.
//!
//! Everything here compiles for native and `wasm32`, so the simulator, the backend and the
//! UI all use the same codec, delivery tracking, geometry and planning code.

pub mod client;
pub mod codec;
pub mod geo;
pub mod kinematics;
pub mod lidar;
pub mod reliable;
pub mod survey;
pub mod terrain;

/// Generated struct-frame message types.
pub use struct_frame_sdk as msgs;

pub use client::Client;
pub use codec::{Decoder, Message};
pub use geo::{LatLon, LocalFrame, Vec2};
