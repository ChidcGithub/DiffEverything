# Serialization fixtures

- `old.cbor` / `new.cbor` were generated with Python `cbor2`.
- `old.msgpack` / `new.msgpack` were generated with Python `msgpack`.
- `old.pb` / `new.pb` are hand-encoded protobuf wire messages with fields
  `id`, `name`, `score`, `count` and `note`.

Each pair has at least five semantic changes so concise/full detail modes can
be compared.
