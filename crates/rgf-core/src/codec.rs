//! The only module that talks to the generated struct-frame framing code.
//!
//! Every link in the system (UI ↔ backend, backend ↔ vehicle) carries frames in the Bulk
//! profile: lidar scans and video chunks need its 16-bit length, and message IDs carry a
//! package byte.

use struct_frame_sdk::{
    common, control, mission, params, rtk, sensors, simctl, telemetry, AccumulatingReader, BufferWriter,
    StructFrameMessage, PROFILE_BULK_CONFIG,
};

/// Largest frame on the wire (a full video chunk) with headroom.
const MAX_FRAME: usize = 8010 + 64;
const READER_CAPACITY: usize = 256 * 1024;

/// Encode one message as a complete frame.
pub fn encode<M: StructFrameMessage>(msg: &M) -> Vec<u8> {
    let mut writer = BufferWriter::new(PROFILE_BULK_CONFIG, MAX_FRAME);
    writer.write(msg);
    writer.data().to_vec()
}

macro_rules! messages {
    ($($variant:ident => $ty:path),* $(,)?) => {
        /// Any message of the protocol. Variants are boxed because the sensor messages
        /// carry kilobyte-sized inline arrays.
        #[derive(Debug, Clone)]
        pub enum Message {
            $($variant(Box<$ty>)),*
        }

        impl Message {
            /// Wire ID: `(package id << 8) | message id`.
            pub fn msg_id(&self) -> u16 {
                match self { $(Message::$variant(_) => <$ty as StructFrameMessage>::MSG_ID),* }
            }

            pub fn name(&self) -> &'static str {
                match self { $(Message::$variant(_) => stringify!($variant)),* }
            }

            pub fn encode(&self) -> Vec<u8> {
                match self { $(Message::$variant(m) => encode(&**m)),* }
            }

            fn decode(msg_id: u16, payload: &[u8]) -> Option<Message> {
                match msg_id {
                    $(<$ty as StructFrameMessage>::MSG_ID =>
                        <$ty>::unpack(payload).map(|m| Message::$variant(Box::new(m))),)*
                    _ => None,
                }
            }
        }

        $(impl From<$ty> for Message {
            fn from(m: $ty) -> Self { Message::$variant(Box::new(m)) }
        })*
    };
}

messages! {
    Heartbeat => common::Heartbeat,
    VehicleInfo => common::VehicleInfo,
    ActuatorInfo => common::ActuatorInfo,
    CommandAck => common::CommandAck,
    Pose => telemetry::Pose,
    MotorStatus => telemetry::MotorStatus,
    PowerStatus => telemetry::PowerStatus,
    NavStatus => telemetry::NavStatus,
    SetMode => control::SetMode,
    DriveCommand => control::DriveCommand,
    MotorCommand => control::MotorCommand,
    EStop => control::EStop,
    ParamRequestList => params::ParamRequestList,
    ParamSet => params::ParamSet,
    ParamValue => params::ParamValue,
    MissionCount => mission::MissionCount,
    MissionItem => mission::MissionItem,
    MissionCommand => mission::MissionCommand,
    LidarScan => sensors::LidarScan,
    VideoFrame => sensors::VideoFrame,
    BaseStatus => rtk::BaseStatus,
    BaseConfig => rtk::BaseConfig,
    RtcmData => rtk::RtcmData,
    SimSelectVehicle => simctl::SimSelectVehicle,
    SimLinkFault => simctl::SimLinkFault,
}

impl Message {
    /// Sequence number of a message whose delivery must be acknowledged, `None` for
    /// telemetry and streamed commands.
    pub fn seq(&self) -> Option<u16> {
        Some(match self {
            Message::SetMode(m) => m.seq,
            Message::EStop(m) => m.seq,
            Message::ParamRequestList(m) => m.seq,
            Message::ParamSet(m) => m.seq,
            Message::MissionCount(m) => m.seq,
            Message::MissionItem(m) => m.seq,
            Message::MissionCommand(m) => m.seq,
            Message::BaseConfig(m) => m.seq,
            Message::SimSelectVehicle(m) => m.seq,
            Message::SimLinkFault(m) => m.seq,
            _ => return None,
        })
    }
}

/// Streaming decoder: feed it bytes as they arrive, get whole messages back. Corrupt
/// frames are skipped and counted.
pub struct Decoder {
    reader: AccumulatingReader,
    pub bad_frames: u32,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            reader: AccumulatingReader::with_message_info(
                PROFILE_BULK_CONFIG,
                READER_CAPACITY,
                struct_frame_sdk::get_message_info,
            ),
            bad_frames: 0,
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Message> {
        let mut out = Vec::new();
        // The reader drops what does not fit, so hand it pieces it can always take.
        for piece in bytes.chunks(READER_CAPACITY / 4) {
            self.reader.add_data(piece);
            while let Some(frame) = self.reader.next_frame() {
                if !frame.valid {
                    self.bad_frames += 1;
                    continue;
                }
                match Message::decode(frame.msg_id, &frame.msg_data) {
                    Some(msg) => out.push(msg),
                    None => self.bad_frames += 1,
                }
            }
        }
        out
    }
}

/// Read a fixed-size, null-padded string field.
pub fn str_from(field: &[u8]) -> String {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

/// Fill a fixed-size string field, truncating if needed.
pub fn str_to<const N: usize>(s: &str) -> [u8; N] {
    let mut out = [0u8; N];
    let n = s.len().min(N);
    out[..n].copy_from_slice(&s.as_bytes()[..n]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_across_split_reads() {
        let mut scan = sensors::LidarScan { layers: 8, points_per_layer: 180, ..Default::default() };
        scan.ranges_cm_count = 1440;
        for i in 0..1440 {
            scan.ranges_cm[i] = i as u16;
        }
        let msgs: Vec<Message> = vec![
            telemetry::Pose { lat: 41.9, lon: -93.55, speed_mps: 1.5, ..Default::default() }.into(),
            scan.into(),
            params::ParamSet { seq: 7, id: 3, value: 2.5 }.into(),
            common::VehicleInfo { name: str_to("Rover"), ..Default::default() }.into(),
        ];
        let stream: Vec<u8> = msgs.iter().flat_map(|m| m.encode()).collect();

        let mut decoder = Decoder::new();
        let mut got = Vec::new();
        for piece in stream.chunks(97) {
            got.extend(decoder.feed(piece));
        }
        assert_eq!(decoder.bad_frames, 0);
        assert_eq!(got.len(), 4);
        match (&got[0], &got[1], &got[2], &got[3]) {
            (Message::Pose(p), Message::LidarScan(s), Message::ParamSet(ps), Message::VehicleInfo(v)) => {
                assert_eq!(p.lat, 41.9);
                assert_eq!(s.ranges_cm_count, 1440);
                assert_eq!(s.ranges_cm[1439], 1439);
                assert_eq!((ps.seq, ps.id, ps.value), (7, 3, 2.5));
                assert_eq!(str_from(&v.name), "Rover");
            }
            other => panic!("unexpected order: {other:?}"),
        }
        assert_eq!(got[2].seq(), Some(7));
        assert_eq!(got[0].seq(), None);
    }

    #[test]
    fn corrupt_frame_is_skipped_and_stream_recovers() {
        let mut bad = Message::from(control::EStop { seq: 1, engage: true }).encode();
        let last = bad.len() - 1;
        bad[last] ^= 0xFF;
        let good = Message::from(control::EStop { seq: 2, engage: false }).encode();

        let mut decoder = Decoder::new();
        let mut got = decoder.feed(&bad);
        got.extend(decoder.feed(&good));
        assert_eq!(got.len(), 1);
        assert!(matches!(&got[0], Message::EStop(e) if e.seq == 2));
        assert!(decoder.bad_frames >= 1);
    }

    #[test]
    fn every_message_type_round_trips() {
        let all: Vec<Message> = vec![
            common::Heartbeat::default().into(),
            common::VehicleInfo::default().into(),
            common::ActuatorInfo::default().into(),
            common::CommandAck::default().into(),
            telemetry::Pose::default().into(),
            telemetry::MotorStatus::default().into(),
            telemetry::PowerStatus::default().into(),
            telemetry::NavStatus::default().into(),
            control::SetMode::default().into(),
            control::DriveCommand::default().into(),
            control::MotorCommand::default().into(),
            control::EStop::default().into(),
            params::ParamRequestList::default().into(),
            params::ParamSet::default().into(),
            params::ParamValue::default().into(),
            mission::MissionCount::default().into(),
            mission::MissionItem::default().into(),
            mission::MissionCommand::default().into(),
            sensors::LidarScan::default().into(),
            sensors::VideoFrame::default().into(),
            rtk::BaseStatus::default().into(),
            rtk::BaseConfig::default().into(),
            rtk::RtcmData::default().into(),
            simctl::SimSelectVehicle::default().into(),
            simctl::SimLinkFault::default().into(),
        ];
        let mut decoder = Decoder::new();
        for msg in &all {
            let got = decoder.feed(&msg.encode());
            assert_eq!(got.len(), 1, "{} did not decode", msg.name());
            assert_eq!(got[0].msg_id(), msg.msg_id());
        }
    }
}
