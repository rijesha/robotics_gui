// Auto-generated lib.rs for struct-frame SDK
// DO NOT EDIT - regenerate using struct-frame

#![allow(unused_imports, dead_code, non_camel_case_types)]

// Boilerplate SDK modules
pub mod frame_base;
pub mod frame_headers;
pub mod payload_types;
pub mod frame_profiles;
pub mod struct_frame_sdk;

// Re-export main SDK items at the crate root for backward compatibility.
// Prefer `use your_crate::prelude::*;` in new code.
pub use frame_base::{
    fletcher_checksum, fletcher_checksum_ext, FrameChecksum, FrameMsgInfo,
    FrameMsgStatus, MessageInfo, ParserDiagnostics, StructFrameMessage,
};
pub use frame_profiles::{
    AccumulatingReader, BufferReader, BufferWriter, Frames, MessageInfoFn, ProfileConfig,
    PROFILE_BULK_CONFIG, PROFILE_IPC_CONFIG, PROFILE_NETWORK_CONFIG,
    PROFILE_SENSOR_CONFIG, PROFILE_STANDARD_CONFIG,
    encode_message, encode_message_crc, encode_message_crc_ext, encode_message_minimal,
    encode_minimal, encode_with_crc, encode_with_crc_ext, parse_frame, parse_minimal,
    parse_with_crc,
    new_bulk_reader, new_bulk_writer, new_ipc_reader, new_ipc_writer,
    new_network_reader, new_network_writer, new_sensor_reader, new_sensor_writer,
    new_standard_reader, new_standard_writer,
};
pub use struct_frame_sdk::{MessageHandler, SendResult, Subscription, StructFrameSdk};

/// Convenience prelude — preferred import for new code.
/// ```rust
/// use your_crate::prelude::*;
/// ```
pub mod prelude {
    pub use crate::frame_base::{
        fletcher_checksum, fletcher_checksum_ext, FrameChecksum, FrameMsgInfo,
        FrameMsgStatus, MessageInfo, ParserDiagnostics, StructFrameMessage,
    };
    pub use crate::frame_profiles::{
        AccumulatingReader, BufferReader, BufferWriter, Frames, MessageInfoFn, ProfileConfig,
        PROFILE_BULK_CONFIG, PROFILE_IPC_CONFIG, PROFILE_NETWORK_CONFIG,
        PROFILE_SENSOR_CONFIG, PROFILE_STANDARD_CONFIG,
        encode_message, encode_message_crc, encode_message_crc_ext, encode_message_minimal,
        encode_minimal, encode_with_crc, encode_with_crc_ext, parse_frame, parse_minimal,
        parse_with_crc,
        new_bulk_reader, new_bulk_writer, new_ipc_reader, new_ipc_writer,
        new_network_reader, new_network_writer, new_sensor_reader, new_sensor_writer,
        new_standard_reader, new_standard_writer,
    };
    pub use crate::struct_frame_sdk::{MessageHandler, SendResult, Subscription, StructFrameSdk};
}

// Generated message modules
#[path = "common.structframe.rs"]
pub mod common;
#[path = "control.structframe.rs"]
pub mod control;
#[path = "mission.structframe.rs"]
pub mod mission;
#[path = "params.structframe.rs"]
pub mod params;
#[path = "rgf.structframe.rs"]
pub mod rgf;
#[path = "rtk.structframe.rs"]
pub mod rtk;
#[path = "sensors.structframe.rs"]
pub mod sensors;
#[path = "simctl.structframe.rs"]
pub mod simctl;
#[path = "telemetry.structframe.rs"]
pub mod telemetry;

// Merged get_message_info that queries all packages
pub fn get_message_info(msg_id: u16) -> Option<MessageInfo> {
    if let Some(info) = common::get_message_info(msg_id) {
        return Some(info);
    }
    if let Some(info) = control::get_message_info(msg_id) {
        return Some(info);
    }
    if let Some(info) = mission::get_message_info(msg_id) {
        return Some(info);
    }
    if let Some(info) = params::get_message_info(msg_id) {
        return Some(info);
    }
    if let Some(info) = rgf::get_message_info(msg_id) {
        return Some(info);
    }
    if let Some(info) = rtk::get_message_info(msg_id) {
        return Some(info);
    }
    if let Some(info) = sensors::get_message_info(msg_id) {
        return Some(info);
    }
    if let Some(info) = simctl::get_message_info(msg_id) {
        return Some(info);
    }
    if let Some(info) = telemetry::get_message_info(msg_id) {
        return Some(info);
    }
    None
}
