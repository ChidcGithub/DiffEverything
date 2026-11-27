# Tabular fixtures

- `old.parquet` / `new.parquet` were generated with `pyarrow` (Snappy).
- `old.avro` / `new.avro` were generated with `fastavro`.
- `old.orc` / `new.orc` were generated with `pyorc` (Zlib).

All three share the same `id`, `name` and `score` columns with changed,
added and removed values.
