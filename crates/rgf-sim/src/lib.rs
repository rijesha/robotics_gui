//! Simulated vehicles for testing the ground station without hardware.
//!
//! [`Sim`] plays the part of vehicle firmware: it takes struct-frame bytes in, steps a
//! vehicle model, and produces struct-frame bytes out. It does no IO itself, so tests can
//! drive it directly; [`udp`] wraps it in a socket loop.

pub mod camera;
pub mod sim;
pub mod udp;
pub mod vehicles;
pub mod world;

pub use rgf_core::msgs::common::VehicleType;
pub use sim::Sim;

/// Parse a vehicle name as used on the command line.
pub fn parse_vehicle(name: &str) -> Option<VehicleType> {
    match name.to_ascii_lowercase().as_str() {
        "rover6" | "rover" => Some(VehicleType::ROVER6),
        "tractor" => Some(VehicleType::TRACTOR),
        "skid" | "skid_steer" | "skidsteer" => Some(VehicleType::SKID_STEER),
        "boat" => Some(VehicleType::BOAT),
        _ => None,
    }
}
