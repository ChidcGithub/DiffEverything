//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn rejects_invalid_parquet_avro_and_orc() {
    assert!(parse_parquet(b"not parquet").is_err());
    assert!(parse_avro(b"not avro").is_err());
    assert!(parse_orc(b"not orc").is_err());
}
