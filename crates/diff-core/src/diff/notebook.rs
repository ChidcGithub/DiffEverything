use crate::model::{NotebookCellDiff, NotebookDiff};
use serde_json::{Map, Value, json};
use similar::{Algorithm, DiffOp, capture_diff_slices};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
struct Cell {
    id: Option<String>,
    cell_type: String,
    source: String,
    outputs: Value,
    execution_count: Value,
    value: Value,
}

#[derive(Debug, Clone)]
struct Notebook {
    value: Value,
    cells: Vec<Cell>,
}

#[derive(Debug, Clone)]
pub struct NotebookMergeResult {
    pub text: String,
    pub clean: bool,
}

pub fn compare_notebooks(
    left: &[u8],
    right: &[u8],
    max_changes: usize,
) -> std::result::Result<NotebookDiff, String> {
    let left = parse_notebook(left)?;
    let right = parse_notebook(right)?;
    if has_unique_cell_ids(&left) && has_unique_cell_ids(&right) {
        return Ok(compare_notebooks_by_id(&left, &right, max_changes));
    }
    let left_signatures: Vec<String> = left.cells.iter().map(signature).collect();
    let right_signatures: Vec<String> = right.cells.iter().map(signature).collect();
    let ops = capture_diff_slices(Algorithm::Myers, &left_signatures, &right_signatures);

    let mut cells_added = 0;
    let mut cells_removed = 0;
    let mut changed_cells = Vec::new();
    let mut truncated = false;

    for op in ops {
        match op {
            DiffOp::Equal { .. } => {}
            DiffOp::Insert { new_len, .. } => cells_added += new_len,
            DiffOp::Delete { old_len, .. } => cells_removed += old_len,
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                let paired = old_len.min(new_len);
                for offset in 0..paired {
                    if changed_cells.len() >= max_changes {
                        truncated = true;
                        break;
                    }
                    let old_cell = &left.cells[old_index + offset];
                    let new_cell = &right.cells[new_index + offset];
                    if let Some(change) = compare_cell(old_cell, new_cell, new_index + offset + 1) {
                        changed_cells.push(change);
                    }
                }
                cells_removed += old_len.saturating_sub(paired);
                cells_added += new_len.saturating_sub(paired);
            }
        }
    }

    Ok(NotebookDiff {
        left_cells: left.cells.len(),
        right_cells: right.cells.len(),
        cells_added,
        cells_removed,
        cells_moved: moved_cells(&left, &right),
        changed_cells,
        truncated,
    })
}

fn compare_notebooks_by_id(left: &Notebook, right: &Notebook, max_changes: usize) -> NotebookDiff {
    let right_index: BTreeMap<&str, (usize, &Cell)> = right
        .cells
        .iter()
        .enumerate()
        .filter_map(|(index, cell)| cell.id.as_deref().map(|id| (id, (index, cell))))
        .collect();
    let left_ids: BTreeSet<&str> = left
        .cells
        .iter()
        .filter_map(|cell| cell.id.as_deref())
        .collect();

    let mut cells_removed = 0;
    let mut changed_cells = Vec::new();
    let mut truncated = false;
    for old_cell in &left.cells {
        let Some(id) = old_cell.id.as_deref() else {
            continue;
        };
        let Some((index, new_cell)) = right_index.get(id) else {
            cells_removed += 1;
            continue;
        };
        if let Some(change) = compare_cell(old_cell, new_cell, index + 1) {
            if changed_cells.len() < max_changes {
                changed_cells.push(change);
            } else {
                truncated = true;
            }
        }
    }

    let cells_added = right
        .cells
        .iter()
        .filter(|cell| cell.id.as_deref().is_some_and(|id| !left_ids.contains(id)))
        .count();

    NotebookDiff {
        left_cells: left.cells.len(),
        right_cells: right.cells.len(),
        cells_added,
        cells_removed,
        cells_moved: moved_cells(left, right),
        changed_cells,
        truncated,
    }
}

fn has_unique_cell_ids(notebook: &Notebook) -> bool {
    let mut ids = BTreeSet::new();
    notebook
        .cells
        .iter()
        .all(|cell| cell.id.as_deref().is_some_and(|id| ids.insert(id)))
}

fn moved_cells(left: &Notebook, right: &Notebook) -> usize {
    if !has_unique_cell_ids(left) || !has_unique_cell_ids(right) {
        let left_signatures: Vec<String> = left.cells.iter().map(signature).collect();
        let right_signatures: Vec<String> = right.cells.iter().map(signature).collect();
        let left_refs: Vec<&str> = left_signatures.iter().map(String::as_str).collect();
        let right_refs: Vec<&str> = right_signatures.iter().map(String::as_str).collect();
        let common = multiset_common_count(&left_signatures, &right_signatures);
        return common.saturating_sub(lcs_len(&left_refs, &right_refs));
    }
    let left_ids: Vec<&str> = left
        .cells
        .iter()
        .filter_map(|cell| cell.id.as_deref())
        .collect();
    let right_ids: Vec<&str> = right
        .cells
        .iter()
        .filter_map(|cell| cell.id.as_deref())
        .collect();
    if left_ids.is_empty() || right_ids.is_empty() {
        return 0;
    }
    let common = left_ids.iter().filter(|id| right_ids.contains(id)).count();
    common.saturating_sub(lcs_len(&left_ids, &right_ids))
}

fn multiset_common_count(left: &[String], right: &[String]) -> usize {
    let mut left_sorted: Vec<&str> = left.iter().map(String::as_str).collect();
    let mut right_sorted: Vec<&str> = right.iter().map(String::as_str).collect();
    left_sorted.sort_unstable();
    right_sorted.sort_unstable();
    let mut left_index = 0;
    let mut right_index = 0;
    let mut common = 0;
    while left_index < left_sorted.len() && right_index < right_sorted.len() {
        match left_sorted[left_index].cmp(right_sorted[right_index]) {
            std::cmp::Ordering::Less => left_index += 1,
            std::cmp::Ordering::Greater => right_index += 1,
            std::cmp::Ordering::Equal => {
                common += 1;
                left_index += 1;
                right_index += 1;
            }
        }
    }
    common
}

fn lcs_len(left: &[&str], right: &[&str]) -> usize {
    let mut previous = vec![0usize; right.len() + 1];
    let mut current = vec![0usize; right.len() + 1];
    for left_id in left {
        for (index, right_id) in right.iter().enumerate() {
            current[index + 1] = if left_id == right_id {
                previous[index] + 1
            } else {
                previous[index + 1].max(current[index])
            };
        }
        std::mem::swap(&mut previous, &mut current);
        current.fill(0);
    }
    previous[right.len()]
}

fn compare_cell(old: &Cell, new: &Cell, index: usize) -> Option<NotebookCellDiff> {
    let source_changed = old.source != new.source;
    let outputs_changed = old.outputs != new.outputs;
    let execution_changed = old.execution_count != new.execution_count;
    if !source_changed && !outputs_changed && !execution_changed && old.cell_type == new.cell_type {
        return None;
    }

    let source = source_changed.then(|| {
        let mut diff = crate::diff::text::compare_text(&old.source, &new.source, 3, 100);
        diff.format = "cell".to_string();
        diff.encoding_left = "notebook".to_string();
        diff.encoding_right = "notebook".to_string();
        diff
    });

    Some(NotebookCellDiff {
        index,
        cell_type: new.cell_type.clone(),
        source,
        outputs_changed,
        execution_changed,
    })
}

pub fn merge_notebooks(
    base: &[u8],
    ours: &[u8],
    theirs: &[u8],
) -> std::result::Result<NotebookMergeResult, String> {
    let base = parse_notebook(base)?;
    let ours = parse_notebook(ours)?;
    let theirs = parse_notebook(theirs)?;

    let all_have_ids = base.cells.iter().all(|cell| cell.id.is_some())
        && ours.cells.iter().all(|cell| cell.id.is_some())
        && theirs.cells.iter().all(|cell| cell.id.is_some());

    let (merged_cells, clean) = if all_have_ids {
        merge_by_id(&base, &ours, &theirs)?
    } else {
        merge_by_index(&base, &ours, &theirs)?
    };

    let mut root = ours.value.clone();
    let object = root
        .as_object_mut()
        .ok_or_else(|| "notebook root is not an object".to_string())?;
    object.insert("cells".to_string(), Value::Array(merged_cells));
    let text = serde_json::to_string_pretty(&root)
        .map_err(|error| format!("cannot serialize notebook: {error}"))?;
    Ok(NotebookMergeResult { text, clean })
}

fn merge_by_id(
    base: &Notebook,
    ours: &Notebook,
    theirs: &Notebook,
) -> std::result::Result<(Vec<Value>, bool), String> {
    let base_map = id_map(base);
    let ours_map = id_map(ours);
    let theirs_map = id_map(theirs);
    let mut clean = true;
    let mut merged: BTreeMap<String, Value> = BTreeMap::new();

    for base_cell in &base.cells {
        let Some(id) = &base_cell.id else {
            continue;
        };
        let ours_cell = ours_map.get(id);
        let theirs_cell = theirs_map.get(id);
        match (ours_cell, theirs_cell) {
            (None, None) => {}
            (None, Some(theirs_cell)) => {
                if !cell_equal(base_cell, theirs_cell) {
                    clean = false;
                }
                merged.insert(id.clone(), theirs_cell.value.clone());
            }
            (Some(ours_cell), None) => {
                if !cell_equal(base_cell, ours_cell) {
                    clean = false;
                }
                merged.insert(id.clone(), ours_cell.value.clone());
            }
            (Some(ours_cell), Some(theirs_cell)) => {
                let (cell, cell_clean) = merge_cell(base_cell, ours_cell, theirs_cell)?;
                merged.insert(id.clone(), cell);
                clean &= cell_clean;
            }
        }
    }

    for (id, ours_cell) in &ours_map {
        if base_map.contains_key(id) {
            continue;
        }
        if let Some(theirs_cell) = theirs_map.get(id)
            && !cell_equal(ours_cell, theirs_cell)
        {
            merged.insert(id.clone(), merge_added_cell(ours_cell, theirs_cell));
            clean = false;
            continue;
        }
        merged.insert(id.clone(), ours_cell.value.clone());
    }
    for (id, theirs_cell) in &theirs_map {
        if base_map.contains_key(id) || ours_map.contains_key(id) {
            continue;
        }
        merged.insert(id.clone(), theirs_cell.value.clone());
    }

    let base_common: Vec<String> = base
        .cells
        .iter()
        .filter_map(|cell| cell.id.clone())
        .filter(|id| {
            ours_map.contains_key(id) && theirs_map.contains_key(id) && merged.contains_key(id)
        })
        .collect();
    let ours_common: Vec<String> = ours
        .cells
        .iter()
        .filter_map(|cell| cell.id.clone())
        .filter(|id| base_map.contains_key(id) && merged.contains_key(id))
        .collect();
    let theirs_common: Vec<String> = theirs
        .cells
        .iter()
        .filter_map(|cell| cell.id.clone())
        .filter(|id| base_map.contains_key(id) && merged.contains_key(id))
        .collect();
    let ours_reordered = ours_common != base_common;
    let theirs_reordered = theirs_common != base_common;
    if ours_reordered && theirs_reordered && ours_common != theirs_common {
        clean = false;
    }

    let mut order: Vec<String> = if ours_reordered {
        ours.cells
            .iter()
            .filter_map(|cell| cell.id.clone())
            .filter(|id| merged.contains_key(id))
            .collect()
    } else if theirs_reordered {
        theirs
            .cells
            .iter()
            .filter_map(|cell| cell.id.clone())
            .filter(|id| merged.contains_key(id))
            .collect()
    } else {
        base.cells
            .iter()
            .filter_map(|cell| cell.id.clone())
            .filter(|id| merged.contains_key(id))
            .collect()
    };

    for cell in &ours.cells {
        if let Some(id) = &cell.id
            && merged.contains_key(id)
            && !order.contains(id)
        {
            order.push(id.clone());
        }
    }
    for cell in &theirs.cells {
        if let Some(id) = &cell.id
            && merged.contains_key(id)
            && !order.contains(id)
        {
            order.push(id.clone());
        }
    }
    for id in merged.keys() {
        if !order.contains(id) {
            order.push(id.clone());
        }
    }

    let cells = order
        .into_iter()
        .filter_map(|id| merged.remove(&id))
        .collect();
    Ok((cells, clean))
}

fn merge_added_cell(ours: &Cell, theirs: &Cell) -> Value {
    let source = diffy::merge("", &ours.source, &theirs.source).unwrap_or_else(|conflict| conflict);
    let mut cell = ours.value.clone();
    if let Some(object) = cell.as_object_mut() {
        object.insert("source".to_string(), source_value(&ours.value, &source));
    }
    cell
}

fn merge_by_index(
    base: &Notebook,
    ours: &Notebook,
    theirs: &Notebook,
) -> std::result::Result<(Vec<Value>, bool), String> {
    let count = base
        .cells
        .len()
        .max(ours.cells.len())
        .max(theirs.cells.len());
    let mut clean = true;
    let mut cells = Vec::new();
    for index in 0..count {
        match (
            base.cells.get(index),
            ours.cells.get(index),
            theirs.cells.get(index),
        ) {
            (Some(base_cell), Some(ours_cell), Some(theirs_cell)) => {
                let (cell, cell_clean) = merge_cell(base_cell, ours_cell, theirs_cell)?;
                cells.push(cell);
                clean &= cell_clean;
            }
            (None, Some(ours_cell), Some(theirs_cell)) => {
                if cell_equal(ours_cell, theirs_cell) {
                    cells.push(ours_cell.value.clone());
                } else {
                    clean = false;
                    cells.push(ours_cell.value.clone());
                }
            }
            (Some(base_cell), Some(ours_cell), None) => {
                if !cell_equal(base_cell, ours_cell) {
                    clean = false;
                }
                cells.push(ours_cell.value.clone());
            }
            (Some(base_cell), None, Some(theirs_cell)) => {
                if !cell_equal(base_cell, theirs_cell) {
                    clean = false;
                }
                cells.push(theirs_cell.value.clone());
            }
            (_, Some(ours_cell), None) => cells.push(ours_cell.value.clone()),
            (_, None, Some(theirs_cell)) => cells.push(theirs_cell.value.clone()),
            (_, None, None) => {}
        }
    }
    Ok((cells, clean))
}

fn merge_cell(
    base: &Cell,
    ours: &Cell,
    theirs: &Cell,
) -> std::result::Result<(Value, bool), String> {
    let mut clean = true;

    let source = match diffy::merge(&base.source, &ours.source, &theirs.source) {
        Ok(text) => text,
        Err(text) => {
            clean = false;
            text
        }
    };

    let outputs = if ours.outputs == base.outputs {
        theirs.outputs.clone()
    } else if theirs.outputs == base.outputs || ours.outputs == theirs.outputs {
        ours.outputs.clone()
    } else {
        clean = false;
        ours.outputs.clone()
    };

    let execution_count = if ours.execution_count == base.execution_count {
        theirs.execution_count.clone()
    } else if theirs.execution_count == base.execution_count
        || ours.execution_count == theirs.execution_count
    {
        ours.execution_count.clone()
    } else {
        clean = false;
        ours.execution_count.clone()
    };

    let mut cell = ours.value.clone();
    let object = cell
        .as_object_mut()
        .ok_or_else(|| "notebook cell is not an object".to_string())?;
    object.insert("source".to_string(), source_value(&ours.value, &source));
    if object.contains_key("outputs") {
        object.insert("outputs".to_string(), outputs);
    }
    if object.contains_key("execution_count") {
        object.insert("execution_count".to_string(), execution_count);
    }
    if ours.cell_type != base.cell_type {
        object.insert(
            "cell_type".to_string(),
            Value::String(ours.cell_type.clone()),
        );
    } else if theirs.cell_type != base.cell_type {
        object.insert(
            "cell_type".to_string(),
            Value::String(theirs.cell_type.clone()),
        );
    }

    Ok((cell, clean))
}

fn source_value(original: &Value, source: &str) -> Value {
    if original
        .get("source")
        .map(Value::is_string)
        .unwrap_or(false)
    {
        Value::String(source.to_string())
    } else {
        Value::Array(
            source
                .split_inclusive('\n')
                .map(|line| Value::String(line.to_string()))
                .collect(),
        )
    }
}

fn parse_notebook(bytes: &[u8]) -> std::result::Result<Notebook, String> {
    let value: Value = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let cells = value
        .get("cells")
        .and_then(Value::as_array)
        .ok_or_else(|| "notebook has no cells array".to_string())?;
    let mut parsed = Vec::with_capacity(cells.len());
    for cell in cells {
        parsed.push(parse_cell(cell)?);
    }
    Ok(Notebook {
        value,
        cells: parsed,
    })
}

fn parse_cell(value: &Value) -> std::result::Result<Cell, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "notebook cell is not an object".to_string())?;
    let cell_type = object
        .get("cell_type")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let source_value = object.get("source").cloned().unwrap_or(Value::Null);
    let source = if let Some(text) = source_value.as_str() {
        text.to_string()
    } else {
        source_value
            .as_array()
            .map(|lines| lines.iter().filter_map(Value::as_str).collect::<String>())
            .unwrap_or_default()
    };
    Ok(Cell {
        id: object.get("id").and_then(Value::as_str).map(str::to_string),
        cell_type,
        source,
        outputs: object.get("outputs").cloned().unwrap_or(Value::Null),
        execution_count: object
            .get("execution_count")
            .cloned()
            .unwrap_or(Value::Null),
        value: value.clone(),
    })
}

fn signature(cell: &Cell) -> String {
    format!(
        "{}|{}|{}|{}",
        cell.id.clone().unwrap_or_default(),
        cell.cell_type,
        cell.source,
        cell.outputs
    )
}

fn cell_equal(left: &Cell, right: &Cell) -> bool {
    left.cell_type == right.cell_type
        && left.source == right.source
        && left.outputs == right.outputs
}

fn id_map(notebook: &Notebook) -> BTreeMap<String, Cell> {
    notebook
        .cells
        .iter()
        .filter_map(|cell| cell.id.clone().map(|id| (id, cell.clone())))
        .collect()
}

/// Expose a small helper for tests and callers that want to build a notebook
/// cell programmatically without depending on notebook-specific crates.
pub fn notebook_cell(id: &str, cell_type: &str, source: &str) -> Value {
    let mut object = Map::new();
    object.insert("id".to_string(), Value::String(id.to_string()));
    object.insert(
        "cell_type".to_string(),
        Value::String(cell_type.to_string()),
    );
    object.insert("metadata".to_string(), Value::Object(Map::new()));
    object.insert(
        "source".to_string(),
        Value::Array(
            source
                .split_inclusive('\n')
                .map(|line| Value::String(line.to_string()))
                .collect(),
        ),
    );
    if cell_type == "code" {
        object.insert("execution_count".to_string(), Value::Null);
        object.insert("outputs".to_string(), Value::Array(Vec::new()));
    }
    Value::Object(object)
}

pub fn notebook_json(cells: Vec<Value>) -> Value {
    json!({
        "cells": cells,
        "metadata": {},
        "nbformat": 4,
        "nbformat_minor": 5
    })
}

#[cfg(test)]
mod tests {
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
}
