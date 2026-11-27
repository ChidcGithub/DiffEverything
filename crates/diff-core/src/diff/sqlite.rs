use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};
use serde_json::{Map, Value, json};
use std::fmt::Write as _;

const MAX_ROWS_PER_TABLE: usize = 10_000;

pub fn parse_sqlite(bytes: &[u8]) -> Result<Value, String> {
    let dir = tempfile::tempdir().map_err(|error| error.to_string())?;
    let path = dir.path().join("database.sqlite");
    std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
    let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| error.to_string())?;

    let mut root = Map::new();
    root.insert(
        "schema".to_string(),
        Value::Array(read_schema(&connection)?),
    );

    let table_names: Vec<String> = {
        let mut statement = connection
            .prepare(
                "SELECT name FROM sqlite_master \
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?
    };

    let mut tables = Map::new();
    for name in table_names {
        tables.insert(name.clone(), read_table(&connection, &name)?);
    }
    root.insert("tables".to_string(), Value::Object(tables));
    Ok(Value::Object(root))
}

fn read_schema(connection: &Connection) -> Result<Vec<Value>, String> {
    let mut statement = connection
        .prepare(
            "SELECT type, name, tbl_name, COALESCE(sql, '') FROM sqlite_master \
             WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(json!({
                "type": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "table": row.get::<_, String>(2)?,
                "sql": row.get::<_, String>(3)?,
            }))
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())
}

fn read_table(connection: &Connection, name: &str) -> Result<Value, String> {
    let quoted = format!("\"{}\"", name.replace('"', "\"\""));
    let mut statement = connection
        .prepare(&format!("SELECT * FROM {quoted}"))
        .map_err(|error| error.to_string())?;
    let columns: Vec<String> = statement
        .column_names()
        .iter()
        .map(|column| column.to_string())
        .collect();
    let column_count = columns.len();

    let mut rows = Vec::new();
    let mut truncated = false;
    {
        let mut query = statement.query([]).map_err(|error| error.to_string())?;
        while let Some(row) = query.next().map_err(|error| error.to_string())? {
            if rows.len() >= MAX_ROWS_PER_TABLE {
                truncated = true;
                break;
            }
            let mut values = Vec::with_capacity(column_count);
            for index in 0..column_count {
                values.push(sql_value(row, index)?);
            }
            rows.push(Value::Array(values));
        }
    }

    Ok(json!({
        "columns": columns,
        "rows": rows,
        "truncated": truncated,
    }))
}

fn sql_value(row: &rusqlite::Row<'_>, index: usize) -> Result<Value, String> {
    Ok(
        match row.get_ref(index).map_err(|error| error.to_string())? {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(value) => Value::from(value),
            ValueRef::Real(value) => serde_json::Number::from_f64(value)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            ValueRef::Text(bytes) => Value::String(String::from_utf8_lossy(bytes).into_owned()),
            ValueRef::Blob(bytes) => json!({
                "$blob_hex": bytes_to_hex(bytes),
                "$size": bytes.len(),
            }),
        },
    )
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database(statements: &[&str]) -> Vec<u8> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.sqlite");
        let connection = Connection::open(&path).unwrap();
        for statement in statements {
            connection.execute_batch(statement).unwrap();
        }
        drop(connection);
        std::fs::read(path).unwrap()
    }

    #[test]
    fn extracts_schema_columns_and_rows() {
        let bytes = database(&[
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER, photo BLOB)",
            "INSERT INTO users VALUES (1, 'alice', 30, x'00ff')",
            "CREATE INDEX idx_users_name ON users(name)",
        ]);
        let value = parse_sqlite(&bytes).unwrap();
        assert_eq!(value["tables"]["users"]["columns"][0], "id");
        assert_eq!(value["tables"]["users"]["rows"][0][1], "alice");
        assert_eq!(value["tables"]["users"]["rows"][0][2], 30);
        assert_eq!(value["tables"]["users"]["rows"][0][3]["$size"], 2);
        assert_eq!(value["schema"][0]["type"], "index");
        assert!(value["schema"].as_array().unwrap().iter().any(|entry| {
            entry["name"] == "users" && entry["sql"].as_str().unwrap().contains("CREATE TABLE")
        }));
    }

    #[test]
    fn rejects_non_database_bytes() {
        assert!(parse_sqlite(b"not a sqlite database").is_err());
    }
}
