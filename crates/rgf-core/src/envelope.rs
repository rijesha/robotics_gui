//! Addressing for the link between backend and UI, which carries several vehicles at once.
//!
//! The messages themselves do not say which vehicle they belong to. Every WebSocket
//! message is instead one byte naming the vehicle followed by struct-frame bytes.

pub type VehicleId = u8;

/// Addresses the backend itself (the base station) or, towards vehicles, all of them.
pub const SHARED: VehicleId = 255;

pub fn wrap(id: VehicleId, frame: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(frame.len() + 1);
    bytes.push(id);
    bytes.extend_from_slice(frame);
    bytes
}

pub fn split(bytes: &[u8]) -> Option<(VehicleId, &[u8])> {
    bytes.split_first().map(|(id, frame)| (*id, frame))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_then_split_round_trips() {
        let wrapped = wrap(3, &[9, 8, 7]);
        assert_eq!(split(&wrapped), Some((3, &[9u8, 8, 7][..])));
        assert_eq!(split(&wrap(SHARED, &[])), Some((SHARED, &[][..])));
    }

    #[test]
    fn empty_input_has_no_address() {
        assert_eq!(split(&[]), None);
    }
}
