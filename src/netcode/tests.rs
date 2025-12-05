use super::NetworkPacket;

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
