//! Message routing between UIs and the vehicle, with retries on the vehicle hop.

use crate::base::{BaseStation, SimBase};
use rgf_core::msgs::common::{AckResult, CommandAck};
use rgf_core::msgs::rtk::RtcmData;
use rgf_core::reliable::{Action, Policy, TxTracker};
use rgf_core::{LatLon, Message};

/// Four tries, 400 ms apart, before a command is reported as not delivered.
pub const VEHICLE_POLICY: Policy = Policy { timeout_s: 0.4, max_attempts: 4 };

pub struct Router {
    tracker: TxTracker,
    base: Box<dyn BaseStation>,
    next_base_status: f64,
    to_vehicle: Vec<Vec<u8>>,
    to_ui: Vec<Vec<u8>>,
}

impl Default for Router {
    fn default() -> Self {
        Self::new(Box::new(SimBase::default()))
    }
}

impl Router {
    pub fn new(base: Box<dyn BaseStation>) -> Self {
        Self {
            tracker: TxTracker::new(VEHICLE_POLICY),
            base,
            next_base_status: 0.0,
            to_vehicle: Vec::new(),
            to_ui: Vec::new(),
        }
    }

    pub fn drain_to_vehicle(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.to_vehicle)
    }

    pub fn drain_to_ui(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.to_ui)
    }

    fn settle(&mut self, seq: u16, now: f64) {
        if self.tracker.resolve(seq, Ok(()), now).is_some() {
            self.tracker.begin(&seq.to_string());
        }
    }

    pub fn from_ui(&mut self, msg: Message, now: f64) {
        if let Message::BaseConfig(config) = &msg {
            // The base station lives here, so this is answered locally.
            let ok = self.base.configure(config, now);
            let result = if ok { AckResult::ACCEPTED } else { AckResult::REJECTED };
            self.to_ui.push(Message::from(CommandAck { seq: config.seq, msg_id: msg.msg_id(), result }).encode());
            return;
        }
        let frame = msg.encode();
        if let Some(seq) = msg.seq() {
            let label = seq.to_string();
            self.tracker.begin(&label);
            self.tracker.track(&label, seq, msg.msg_id(), frame.clone(), now);
        }
        self.to_vehicle.push(frame);
    }

    pub fn from_vehicle(&mut self, msg: Message, now: f64) {
        match &msg {
            Message::CommandAck(ack) => self.settle(ack.seq, now),
            // A parameter readback that echoes a sequence number answers that ParamSet.
            Message::ParamValue(value) if value.seq != 0 => self.settle(value.seq, now),
            Message::Pose(pose) => self.base.observe_rover(LatLon::new(pose.lat, pose.lon), now),
            _ => {}
        }
        self.to_ui.push(msg.encode());
    }

    pub fn tick(&mut self, now: f64) {
        for action in self.tracker.poll(now) {
            match action {
                Action::Resend { frame, .. } => self.to_vehicle.push(frame),
                Action::Failed { seq, msg_id } => {
                    self.tracker.begin(&seq.to_string());
                    let ack = CommandAck { seq, msg_id, result: AckResult::TIMEOUT };
                    self.to_ui.push(Message::from(ack).encode());
                }
            }
        }

        self.base.tick(now);
        for rtcm in self.base.take_rtcm() {
            let mut msg = RtcmData::default();
            let n = rtcm.len().min(msg.data.len());
            msg.data[..n].copy_from_slice(&rtcm[..n]);
            msg.data_count = n as u8;
            self.to_vehicle.push(Message::from(msg).encode());
        }
        if now >= self.next_base_status {
            self.next_base_status = now + 1.0;
            self.to_ui.push(Message::from(self.base.status()).encode());
        }
    }
}
