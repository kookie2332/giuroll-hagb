use crate::netcode::{FrameTimeData, NetworkPacket};
use std::time::Instant;

fn build_packet(with_initial: bool) -> NetworkPacket {
    NetworkPacket {
        id: 42,
        desyncdetect: 7,
        delay: 3,
        max_rollback: 9,
        inputs: vec![0x1201, 0x3402, 0xabcd],
        last_confirm: 17,
        sync: Some(-5),
        initial_max_rollback: with_initial.then_some(6),
    }
}

#[test]
fn encode_decode_roundtrip_without_initial_max_rollback() {
    let packet = build_packet(false);

    let encoded = packet.encode();
    // Base layout is 20 bytes plus two bytes per input when the optional byte is absent.
    assert_eq!(encoded.len(), 20 + packet.inputs.len() * 2);

    let decoded = NetworkPacket::decode(&encoded);

    assert_eq!(decoded.id, packet.id);
    assert_eq!(decoded.desyncdetect, packet.desyncdetect);
    assert_eq!(decoded.delay, packet.delay);
    assert_eq!(decoded.max_rollback, packet.max_rollback);
    assert_eq!(decoded.inputs, packet.inputs);
    assert_eq!(decoded.last_confirm, packet.last_confirm);
    assert_eq!(decoded.sync, packet.sync);
    assert_eq!(decoded.initial_max_rollback, None);
}

#[test]
fn encode_decode_roundtrip_with_initial_max_rollback() {
    let packet = build_packet(true);

    let encoded = packet.encode();
    // An extra byte is appended when `initial_max_rollback` is present.
    assert_eq!(encoded.len(), 21 + packet.inputs.len() * 2);

    let decoded = NetworkPacket::decode(&encoded);

    assert_eq!(decoded.id, packet.id);
    assert_eq!(decoded.desyncdetect, packet.desyncdetect);
    assert_eq!(decoded.delay, packet.delay);
    assert_eq!(decoded.max_rollback, packet.max_rollback);
    assert_eq!(decoded.inputs, packet.inputs);
    assert_eq!(decoded.last_confirm, packet.last_confirm);
    assert_eq!(decoded.sync, packet.sync);
    assert_eq!(decoded.initial_max_rollback, packet.initial_max_rollback);
}

#[test]
fn encode_decode_with_no_sync() {
    let packet = NetworkPacket {
        id: 100,
        desyncdetect: 0,
        delay: 0,
        max_rollback: 6,
        inputs: vec![0x0000],
        last_confirm: 99,
        sync: None,
        initial_max_rollback: None,
    };

    let encoded = packet.encode();
    let decoded = NetworkPacket::decode(&encoded);

    assert_eq!(decoded.sync, None);
}

#[test]
fn encode_decode_with_empty_inputs() {
    let packet = NetworkPacket {
        id: 1,
        desyncdetect: 0,
        delay: 2,
        max_rollback: 6,
        inputs: vec![],
        last_confirm: 0,
        sync: Some(1000),
        initial_max_rollback: None,
    };

    let encoded = packet.encode();
    // With no inputs: 20 bytes base
    assert_eq!(encoded.len(), 20);

    let decoded = NetworkPacket::decode(&encoded);
    assert!(decoded.inputs.is_empty());
}

#[test]
fn encode_decode_with_max_values() {
    let packet = NetworkPacket {
        id: usize::MAX,
        desyncdetect: 255,
        delay: 255,
        max_rollback: 255,
        inputs: vec![0xFFFF, 0xFFFF],
        last_confirm: usize::MAX,
        sync: Some(i32::MAX - 1), // Not i32::MAX since that's used for None
        initial_max_rollback: Some(255),
    };

    let encoded = packet.encode();
    let decoded = NetworkPacket::decode(&encoded);

    assert_eq!(decoded.id, packet.id);
    assert_eq!(decoded.desyncdetect, 255);
    assert_eq!(decoded.delay, 255);
    assert_eq!(decoded.max_rollback, 255);
    assert_eq!(decoded.inputs, packet.inputs);
    assert_eq!(decoded.last_confirm, packet.last_confirm);
    assert_eq!(decoded.sync, packet.sync);
    assert_eq!(decoded.initial_max_rollback, Some(255));
}

#[test]
fn encode_decode_with_zero_id() {
    let packet = NetworkPacket {
        id: 0,
        desyncdetect: 0,
        delay: 0,
        max_rollback: 0,
        inputs: vec![0],
        last_confirm: 0,
        sync: Some(0),
        initial_max_rollback: Some(0),
    };

    let encoded = packet.encode();
    let decoded = NetworkPacket::decode(&encoded);

    assert_eq!(decoded.id, 0);
    assert_eq!(decoded.delay, 0);
    assert_eq!(decoded.max_rollback, 0);
}

#[test]
fn test_sync_i32_max_encodes_as_none() {
    // i32::MAX is used internally to represent None
    let packet = NetworkPacket {
        id: 1,
        desyncdetect: 0,
        delay: 0,
        max_rollback: 6,
        inputs: vec![],
        last_confirm: 0,
        sync: None,
        initial_max_rollback: None,
    };

    let encoded = packet.encode();
    // Check that the sync bytes are i32::MAX
    let sync_offset = 12 + 0 * 2 + 4; // after inputs + last_confirm
    let sync_bytes = &encoded[sync_offset..sync_offset + 4];
    let sync_value = i32::from_le_bytes(sync_bytes.try_into().unwrap());
    assert_eq!(sync_value, i32::MAX);
}

#[test]
fn test_frame_time_data_variants() {
    // Test Empty variant
    let empty = FrameTimeData::Empty;
    assert!(matches!(empty, FrameTimeData::Empty));

    // Test LocalFirst variant
    let now = Instant::now();
    let local_first = FrameTimeData::LocalFirst(now);
    assert!(matches!(local_first, FrameTimeData::LocalFirst(_)));

    // Test RemoteFirst variant
    let remote_first = FrameTimeData::RemoteFirst(now);
    assert!(matches!(remote_first, FrameTimeData::RemoteFirst(_)));

    // Test Done variant
    let done = FrameTimeData::Done(12345);
    assert!(matches!(done, FrameTimeData::Done(12345)));
}

#[test]
fn test_frame_time_data_clone() {
    let done = FrameTimeData::Done(-500);
    let cloned = done.clone();
    assert!(matches!(cloned, FrameTimeData::Done(-500)));
}

#[test]
fn test_network_packet_clone() {
    let packet = build_packet(true);
    let cloned = packet.clone();

    assert_eq!(cloned.id, packet.id);
    assert_eq!(cloned.inputs, packet.inputs);
    assert_eq!(cloned.initial_max_rollback, packet.initial_max_rollback);
}

#[test]
fn test_network_packet_debug() {
    let packet = build_packet(false);
    let debug_str = format!("{:?}", packet);

    // Ensure Debug trait is implemented and contains key fields
    assert!(debug_str.contains("NetworkPacket"));
    assert!(debug_str.contains("42")); // id
    assert!(debug_str.contains("7"));  // desyncdetect
}
