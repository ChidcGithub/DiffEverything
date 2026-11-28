//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn parses_cbor_map_array_and_bytes() {
    let bytes = [
        0xa3, 0x61, b'a', 0x01, 0x61, b'b', 0x82, 0xf5, 0x61, b'x', 0x61, b'c', 0x43, 0x00, 0x01,
        0x02,
    ];
    let value = parse_cbor(&bytes).unwrap();
    assert_eq!(value["a"], 1);
    assert_eq!(value["b"][0], true);
    assert_eq!(value["b"][1], "x");
    assert_eq!(value["c"]["$bytes"], "AAEC");
}

#[test]
fn parses_messagepack_map_and_binary() {
    let bytes = [
        0x82, 0xa1, b'a', 0x01, 0xa1, b'b', 0xc4, 0x03, 0x00, 0x01, 0x02,
    ];
    let value = parse_messagepack(&bytes).unwrap();
    assert_eq!(value["a"], 1);
    assert_eq!(value["b"]["$bytes"], "AAEC");
}

#[test]
fn parses_protobuf_scalars_and_nested_messages() {
    let bytes = [
        0x08, 0x96, 0x01, 0x12, 0x02, b'h', b'i', 0x1a, 0x04, 0x08, 0x01, 0x10, 0x02,
    ];
    let value = parse_protobuf(&bytes).unwrap();
    assert_eq!(value["field_1"], 150);
    assert_eq!(value["field_2"], "hi");
    assert_eq!(value["field_3"]["field_1"], 1);
    assert_eq!(value["field_3"]["field_2"], 2);
}
