//! Unit tests for this module, compiled as its child through `#[path]`.
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
