//! Message routing between UIs and the vehicles, with retries on the vehicle hop.

use crate::base::{BaseStation, SimBase};
use rgf_core::envelope::{VehicleId, SHARED};
use rgf_core::msgs::common::{AckResult, CommandAck};
use rgf_core::msgs::rtk::RtcmData;
use rgf_core::reliable::{Action, Policy, TxTracker};
use rgf_core::{LatLon, Message};
use std::collections::BTreeMap;
use std::net::SocketAddr;

/// Four tries, 400 ms apart, before a command is reported as not delivered.
pub const VEHICLE_POLICY: Policy = Policy { timeout_s: 0.4, max_attempts: 4 };

/// Most vehicles one backend will talk to.
pub const MAX_VEHICLES: usize = 32;

/// Which vehicle is which. A vehicle is known by the address its telemetry comes from and
/// numbered in the order it was registered.
#[derive(Debug, Default)]
pub struct Registry {
    addrs: Vec<SocketAddr>,
}

impl Registry {
    /// The id of the vehicle at `addr`, registering it if it is new. `None` once full.
    pub fn id_for(&mut self, addr: SocketAddr) -> Option<VehicleId> {
        if let Some(index) = self.addrs.iter().position(|a| *a == addr) {
            return Some(index as VehicleId);
        }
        if self.addrs.len() >= MAX_VEHICLES {
            return None;
        }
        self.addrs.push(addr);
        Some((self.addrs.len() - 1) as VehicleId)
    }

    pub fn addr(&self, id: VehicleId) -> Option<SocketAddr> {
        self.addrs.get(id as usize).copied()
    }

    pub fn addrs(&self) -> &[SocketAddr] {
        &self.addrs
    }
}

pub struct Router {
    /// Sequence numbers are only unique per vehicle, so each has its own tracker.
    trackers: BTreeMap<VehicleId, TxTracker>,
    base: Box<dyn BaseStation>,
    next_base_status: f64,
    to_vehicle: Vec<(VehicleId, Vec<u8>)>,
    to_ui: Vec<(VehicleId, Vec<u8>)>,
}

impl Default for Router {
    fn default() -> Self {
        Self::new(Box::new(SimBase::default()))
    }
}

impl Router {
    pub fn new(base: Box<dyn BaseStation>) -> Self {
        Self { trackers: BTreeMap::new(), base, next_base_status: 0.0, to_vehicle: Vec::new(), to_ui: Vec::new() }
    }

    /// Frames for vehicles; [`SHARED`] goes to all of them.
    pub fn drain_to_vehicle(&mut self) -> Vec<(VehicleId, Vec<u8>)> {
        std::mem::take(&mut self.to_vehicle)
    }

    /// Frames for UIs, each with the vehicle it came from.
    pub fn drain_to_ui(&mut self) -> Vec<(VehicleId, Vec<u8>)> {
        std::mem::take(&mut self.to_ui)
    }

    fn tracker(&mut self, vehicle: VehicleId) -> &mut TxTracker {
        self.trackers.entry(vehicle).or_insert_with(|| TxTracker::new(VEHICLE_POLICY))
    }

    fn settle(&mut self, vehicle: VehicleId, seq: u16, now: f64) {
        let tracker = self.tracker(vehicle);
        if tracker.resolve(seq, Ok(()), now).is_some() {
            tracker.begin(&seq.to_string());
        }
    }

    pub fn from_ui(&mut self, vehicle: VehicleId, msg: Message, now: f64) {
        if let Message::BaseConfig(config) = &msg {
            // The base station lives here, so this is answered locally.
            let ok = self.base.configure(config, now);
            let result = if ok { AckResult::ACCEPTED } else { AckResult::REJECTED };
            let ack = CommandAck { seq: config.seq, msg_id: msg.msg_id(), result };
            self.to_ui.push((vehicle, Message::from(ack).encode()));
            return;
        }
        let frame = msg.encode();
        if let Some(seq) = msg.seq() {
            let label = seq.to_string();
            let tracker = self.tracker(vehicle);
            tracker.begin(&label);
            tracker.track(&label, seq, msg.msg_id(), frame.clone(), now);
        }
        self.to_vehicle.push((vehicle, frame));
    }

    pub fn from_vehicle(&mut self, vehicle: VehicleId, msg: Message, now: f64) {
        match &msg {
            Message::CommandAck(ack) => self.settle(vehicle, ack.seq, now),
            // A parameter readback that echoes a sequence number answers that ParamSet.
            Message::ParamValue(value) if value.seq != 0 => self.settle(vehicle, value.seq, now),
            // One base serves the whole site, so it is placed by the first vehicle alone.
            Message::Pose(pose) if vehicle == 0 => self.base.observe_rover(LatLon::new(pose.lat, pose.lon), now),
            _ => {}
        }
        self.to_ui.push((vehicle, msg.encode()));
    }

    pub fn tick(&mut self, now: f64) {
        for (&vehicle, tracker) in &mut self.trackers {
            for action in tracker.poll(now) {
                match action {
                    Action::Resend { frame, .. } => self.to_vehicle.push((vehicle, frame)),
                    Action::Failed { seq, msg_id } => {
                        tracker.begin(&seq.to_string());
                        let ack = CommandAck { seq, msg_id, result: AckResult::TIMEOUT };
                        self.to_ui.push((vehicle, Message::from(ack).encode()));
                    }
                }
            }
        }

        self.base.tick(now);
        for rtcm in self.base.take_rtcm() {
            let mut msg = RtcmData::default();
            let n = rtcm.len().min(msg.data.len());
            msg.data[..n].copy_from_slice(&rtcm[..n]);
            msg.data_count = n as u8;
            self.to_vehicle.push((SHARED, Message::from(msg).encode()));
        }
        if now >= self.next_base_status {
            self.next_base_status = now + 1.0;
            self.to_ui.push((SHARED, Message::from(self.base.status()).encode()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vehicles_are_numbered_in_order_of_registration() {
        let addr = |port: u16| SocketAddr::from(([127, 0, 0, 1], port));
        let mut registry = Registry::default();
        assert_eq!(registry.id_for(addr(14560)), Some(0));
        assert_eq!(registry.id_for(addr(14561)), Some(1));
        assert_eq!(registry.id_for(addr(14560)), Some(0));
        assert_eq!(registry.addr(1), Some(addr(14561)));
        assert_eq!(registry.addr(2), None);
        for port in 0..MAX_VEHICLES as u16 {
            registry.id_for(addr(20000 + port));
        }
        assert_eq!(registry.id_for(addr(30000)), None);
    }
}
