//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

fn write_notebook(dir: &std::path::Path, name: &str, cells: Vec<Value>) -> Vec<u8> {
    let value = notebook_json(cells);
    let bytes = serde_json::to_vec_pretty(&value).unwrap();
    std::fs::write(dir.join(name), &bytes).unwrap();
    bytes
}

#[test]
fn compares_cells_by_id() {
    let dir = tempfile::tempdir().unwrap();
    let left = write_notebook(
        dir.path(),
        "left.ipynb",
        vec![
            notebook_cell("a", "markdown", "# Old\n"),
            notebook_cell("b", "code", "value = 1\n"),
        ],
    );
    let right = write_notebook(
        dir.path(),
        "right.ipynb",
        vec![
            notebook_cell("a", "markdown", "# New\n"),
            notebook_cell("b", "code", "value = 1\n"),
            notebook_cell("c", "code", "added = True\n"),
        ],
    );
    let diff = compare_notebooks(&left, &right, 100).unwrap();
    assert_eq!(diff.left_cells, 2);
    assert_eq!(diff.right_cells, 3);
    assert_eq!(diff.cells_added, 1);
    assert_eq!(diff.cells_removed, 0);
    assert_eq!(diff.changed_cells.len(), 1);
    assert_eq!(diff.changed_cells[0].cell_type, "markdown");
    assert!(diff.changed_cells[0].source.is_some());
}

#[test]
fn reports_moved_cells() {
    let dir = tempfile::tempdir().unwrap();
    let left = write_notebook(
        dir.path(),
        "left.ipynb",
        vec![
            notebook_cell("a", "markdown", "a\n"),
            notebook_cell("b", "markdown", "b\n"),
            notebook_cell("c", "markdown", "c\n"),
        ],
    );
    let right = write_notebook(
        dir.path(),
        "right.ipynb",
        vec![
            notebook_cell("c", "markdown", "c\n"),
            notebook_cell("a", "markdown", "a\n"),
            notebook_cell("b", "markdown", "b\n"),
        ],
    );
    let diff = compare_notebooks(&left, &right, 100).unwrap();
    assert!(diff.cells_moved > 0);
    assert_eq!(diff.cells_added, 0);
    assert_eq!(diff.cells_removed, 0);
    assert!(diff.changed_cells.is_empty());
}

#[test]
fn changed_cell_can_move_without_being_added_or_removed() {
    let dir = tempfile::tempdir().unwrap();
    let left = write_notebook(
        dir.path(),
        "left.ipynb",
        vec![
            notebook_cell("a", "code", "value = 1\n"),
            notebook_cell("b", "code", "other = 2\n"),
        ],
    );
    let right = write_notebook(
        dir.path(),
        "right.ipynb",
        vec![
            notebook_cell("b", "code", "other = 2\n"),
            notebook_cell("a", "code", "value = 10\n"),
        ],
    );
    let diff = compare_notebooks(&left, &right, 100).unwrap();
    assert_eq!(diff.cells_added, 0);
    assert_eq!(diff.cells_removed, 0);
    assert_eq!(diff.cells_moved, 1);
    assert_eq!(diff.changed_cells.len(), 1);
    assert_eq!(diff.changed_cells[0].index, 2);
}

#[test]
fn detects_moves_for_cells_without_ids() {
    let dir = tempfile::tempdir().unwrap();
    let mut first = notebook_cell("a", "markdown", "first\n");
    first.as_object_mut().unwrap().remove("id");
    let mut second = notebook_cell("b", "markdown", "second\n");
    second.as_object_mut().unwrap().remove("id");
    let left = write_notebook(
        dir.path(),
        "left.ipynb",
        vec![first.clone(), second.clone()],
    );
    let right = write_notebook(dir.path(), "right.ipynb", vec![second, first]);
    let diff = compare_notebooks(&left, &right, 100).unwrap();
    assert_eq!(diff.cells_moved, 1);
    assert_eq!(diff.cells_added, 1);
    assert_eq!(diff.cells_removed, 1);
}

#[test]
fn merge_preserves_ours_cell_order() {
    let dir = tempfile::tempdir().unwrap();
    let base = write_notebook(
        dir.path(),
        "base.ipynb",
        vec![
            notebook_cell("a", "markdown", "a\n"),
            notebook_cell("b", "markdown", "b\n"),
        ],
    );
    let ours = write_notebook(
        dir.path(),
        "ours.ipynb",
        vec![
            notebook_cell("b", "markdown", "b\n"),
            notebook_cell("a", "markdown", "a\n"),
        ],
    );
    let theirs = write_notebook(
        dir.path(),
        "theirs.ipynb",
        vec![
            notebook_cell("a", "markdown", "a\n"),
            notebook_cell("b", "markdown", "b changed\n"),
        ],
    );
    let merged = merge_notebooks(&base, &ours, &theirs).unwrap();
    assert!(merged.clean);
    let value: Value = serde_json::from_str(&merged.text).unwrap();
    let cells = value["cells"].as_array().unwrap();
    assert_eq!(cells[0]["id"], "b");
    assert!(cells[0]["source"].to_string().contains("b changed"));
    assert_eq!(cells[1]["id"], "a");
}

#[test]
fn merges_non_conflicting_cells() {
    let dir = tempfile::tempdir().unwrap();
    let base = write_notebook(
        dir.path(),
        "base.ipynb",
        vec![
            notebook_cell("a", "markdown", "# Base\n"),
            notebook_cell("b", "code", "x = 1\n"),
        ],
    );
    let ours = write_notebook(
        dir.path(),
        "ours.ipynb",
        vec![
            notebook_cell("a", "markdown", "# Ours\n"),
            notebook_cell("b", "code", "x = 1\n"),
        ],
    );
    let theirs = write_notebook(
        dir.path(),
        "theirs.ipynb",
        vec![
            notebook_cell("a", "markdown", "# Base\n"),
            notebook_cell("b", "code", "x = 2\n"),
        ],
    );
    let merged = merge_notebooks(&base, &ours, &theirs).unwrap();
    assert!(merged.clean);
    let value: Value = serde_json::from_str(&merged.text).unwrap();
    let cells = value["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 2);
    assert!(cells[0]["source"].to_string().contains("Ours"));
    assert!(cells[1]["source"].to_string().contains("x = 2"));
}

#[test]
fn reports_source_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let base = write_notebook(
        dir.path(),
        "base.ipynb",
        vec![notebook_cell("a", "markdown", "base\n")],
    );
    let ours = write_notebook(
        dir.path(),
        "ours.ipynb",
        vec![notebook_cell("a", "markdown", "ours\n")],
    );
    let theirs = write_notebook(
        dir.path(),
        "theirs.ipynb",
        vec![notebook_cell("a", "markdown", "theirs\n")],
    );
    let merged = merge_notebooks(&base, &ours, &theirs).unwrap();
    assert!(!merged.clean);
    assert!(merged.text.contains("<<<<<<< ours"));
    assert!(merged.text.contains(">>>>>>> theirs"));
}
