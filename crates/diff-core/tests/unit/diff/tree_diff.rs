//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

fn rust(source: &str) -> SyntaxNode {
    let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    build_tree(source, &language, TreeOptions::default()).unwrap()
}

#[test]
fn builds_and_compares_trees() {
    let left = rust("fn main() { let value = 1; }");
    let right = rust("fn main() { let value = 2; }");
    let diff = compare_trees("rust", &left, &right, 100);
    assert_eq!(diff.modified_nodes, 1);
    assert!(diff.chunks.iter().any(|chunk| {
        chunk.tag == TreeTag::Modified
            && chunk.path.contains("let_declaration")
            && chunk.text.contains("1 -> 2")
    }));
}

#[test]
fn formatting_only_changes_are_equal() {
    let left = rust("fn main() { let value = 1; }");
    let right = rust("fn main()\n{\n    let value = 1;\n}\n");
    let diff = compare_trees("rust", &left, &right, 100);
    assert_eq!(diff.added_nodes, 0);
    assert_eq!(diff.removed_nodes, 0);
    assert_eq!(diff.modified_nodes, 0);
    assert!(diff.chunks.is_empty());
}

#[test]
fn matches_replacement_siblings_by_similarity() {
    let left = rust("fn main() { let alpha = 1; let beta = 2; }");
    let right = rust("fn main() { let beta = 22; let alpha = 11; }");
    let diff = compare_trees("rust", &left, &right, 100);
    assert_eq!(diff.modified_nodes, 2, "{:?}", diff);
    assert!(
        diff.chunks
            .iter()
            .any(|chunk| chunk.text.contains("1 -> 11")),
        "{:?}",
        diff.chunks
    );
    assert!(
        diff.chunks
            .iter()
            .any(|chunk| chunk.text.contains("2 -> 22")),
        "{:?}",
        diff.chunks
    );
    assert!(
        !diff
            .chunks
            .iter()
            .any(|chunk| chunk.text.contains("alpha -> beta")),
        "{:?}",
        diff.chunks
    );
}

#[test]
fn matches_moved_siblings_across_positions() {
    let left = rust("fn main() { let alpha = 1; let beta = 2; let gamma = 3; }");
    let right = rust("fn main() { let gamma = 3; let alpha = 11; let beta = 22; }");
    let diff = compare_trees("rust", &left, &right, 100);
    assert_eq!(diff.modified_nodes, 2, "{:?}", diff);
    assert!(
        diff.chunks
            .iter()
            .any(|chunk| chunk.text.contains("1 -> 11")),
        "{:?}",
        diff.chunks
    );
    assert!(
        diff.chunks
            .iter()
            .any(|chunk| chunk.text.contains("2 -> 22")),
        "{:?}",
        diff.chunks
    );
    assert!(
        !diff
            .chunks
            .iter()
            .any(|chunk| chunk.text.contains("alpha -> gamma")),
        "{:?}",
        diff.chunks
    );
}

#[test]
fn slider_orders_tied_matches_without_losing_score() {
    let scores = vec![vec![10, 10], vec![10, 10]];
    let mut matches = vec![Some(1), Some(0)];
    slide_ordered_matches(&scores, &mut matches);
    assert_eq!(matches, vec![Some(0), Some(1)]);

    let scores = vec![vec![12, 5], vec![5, 12]];
    let mut matches = vec![Some(1), Some(0)];
    slide_ordered_matches(&scores, &mut matches);
    assert_eq!(matches, vec![Some(1), Some(0)]);
}

#[test]
fn graph_mode_reports_precise_leaf_change() {
    let left = rust("fn main() { let value = 1; }");
    let right = rust("fn main() { let value = 2; }");
    let diff = compare_trees_graph("rust", &left, &right, 100);
    assert_eq!(diff.modified_nodes, 1, "{:?}", diff);
    assert!(
        diff.chunks
            .iter()
            .any(|chunk| chunk.text.contains("1 -> 2"))
    );
    assert_eq!(diff.added_nodes, 0);
    assert_eq!(diff.removed_nodes, 0);
}

#[test]
fn graph_mode_reports_added_statement() {
    let left = rust("fn main() { let value = 1; }");
    let right = rust("fn main() { let value = 1; let extra = 3; }");
    let diff = compare_trees_graph("rust", &left, &right, 100);
    assert!(diff.added_nodes > 0, "{:?}", diff);
    assert_eq!(diff.removed_nodes, 0);
    assert!(diff.chunks.iter().any(|chunk| chunk.tag == TreeTag::Added));
}

#[test]
fn graph_mode_slides_repeated_arguments() {
    let left = rust("fn main() { call(a, a, a); }");
    let right = rust("fn main() { call(a, b, a); }");
    let diff = compare_trees_graph("rust", &left, &right, 100);
    assert_eq!(diff.modified_nodes, 1, "{:?}", diff);
    assert_eq!(diff.added_nodes, 0);
    assert_eq!(diff.removed_nodes, 0);
    assert!(
        diff.chunks
            .iter()
            .any(|chunk| chunk.text.contains("a -> b"))
    );
}

#[test]
fn graph_mode_aligns_inserted_repeated_argument() {
    let left = rust("fn main() { foo(a, b, a); }");
    let right = rust("fn main() { foo(a, a, b, a); }");
    let diff = compare_trees_graph("rust", &left, &right, 100);
    assert_eq!(diff.modified_nodes, 0, "{:?}", diff);
    assert_eq!(diff.removed_nodes, 0, "{:?}", diff);
    assert_eq!(diff.added_nodes, 2, "{:?}", diff);
    assert!(
        diff.chunks
            .iter()
            .any(|chunk| chunk.path.contains("arguments[1]/identifier[1]")),
        "{:?}",
        diff.chunks
    );
}

#[test]
fn graph_mode_flags_reordered_statements() {
    let left = rust("fn main() { foo(); bar(); baz(); }");
    let right = rust("fn main() { baz(); foo(); bar(); }");
    let diff = compare_trees_graph("rust", &left, &right, 100);
    assert!(
        diff.added_nodes + diff.removed_nodes + diff.modified_nodes + diff.moved_nodes > 0,
        "{:?}",
        diff
    );
}

#[test]
fn graph_mode_groups_removed_subtree() {
    let left = rust("fn main() { let value = 1; }");
    let right = rust("fn main() { }");
    let diff = compare_trees_graph("rust", &left, &right, 100);
    let removed: Vec<_> = diff
        .chunks
        .iter()
        .filter(|chunk| chunk.tag == TreeTag::Removed)
        .collect();
    assert_eq!(removed.len(), 1, "{:?}", diff.chunks);
    assert!(removed[0].path.contains("let_declaration"));
    assert!(
        removed[0].text.contains("let value = 1"),
        "{:?}",
        removed[0]
    );
    assert!(diff.removed_nodes > 1, "{:?}", diff);
    assert_eq!(diff.added_nodes, 0);
}

#[test]
fn graph_mode_groups_added_function() {
    let left = rust("fn main() { }");
    let right = rust("fn main() { } fn extra() { let y = 2; }");
    let diff = compare_trees_graph("rust", &left, &right, 100);
    let added: Vec<_> = diff
        .chunks
        .iter()
        .filter(|chunk| chunk.tag == TreeTag::Added)
        .collect();
    assert_eq!(added.len(), 1, "{:?}", diff.chunks);
    assert!(added[0].path.contains("function_item"));
    assert!(added[0].text.contains("fn extra"), "{:?}", added[0]);
    assert!(diff.added_nodes > 1, "{:?}", diff);
    assert_eq!(diff.removed_nodes, 0);
}

#[test]
fn graph_mode_groups_fully_modified_subtree() {
    let left = rust("fn main() { let x = alpha + beta; }");
    let right = rust("fn main() { let x = gamma + delta; }");
    let diff = compare_trees_graph("rust", &left, &right, 100);
    let modified: Vec<_> = diff
        .chunks
        .iter()
        .filter(|chunk| chunk.tag == TreeTag::Modified)
        .collect();
    assert_eq!(modified.len(), 1, "{:?}", diff.chunks);
    assert!(modified[0].path.contains("binary_expression"));
    assert!(
        modified[0].text.contains("alpha + beta -> gamma + delta"),
        "{:?}",
        modified[0]
    );
    assert!(diff.modified_nodes > 1, "{:?}", diff);
}

#[test]
fn graph_mode_reports_moved_statement() {
    let left = rust("fn main() { foo(); bar(); baz(); }");
    let right = rust("fn main() { baz(); foo(); bar(); }");
    let diff = compare_trees_graph("rust", &left, &right, 100);
    let moved: Vec<_> = diff
        .chunks
        .iter()
        .filter(|chunk| chunk.tag == TreeTag::Moved)
        .collect();
    assert_eq!(moved.len(), 1, "{:?}", diff.chunks);
    assert!(moved[0].text.contains("baz"), "{:?}", moved[0]);
    assert!(diff.moved_nodes > 0, "{:?}", diff);
}

#[test]
fn graph_mode_reports_moved_and_modified_statement() {
    let left = rust("fn main() { foo(); bar(); baz(2); }");
    let right = rust("fn main() { baz(3); foo(); bar(); }");
    let diff = compare_trees_graph("rust", &left, &right, 100);
    assert!(diff.moved_nodes > 0, "{:?}", diff);
    assert!(
        diff.added_nodes + diff.removed_nodes + diff.modified_nodes > 0,
        "{:?}",
        diff
    );
    assert!(
        diff.chunks
            .iter()
            .any(|chunk| chunk.tag == TreeTag::Moved && chunk.text.contains("baz")),
        "{:?}",
        diff.chunks
    );
}

#[test]
fn graph_mode_keeps_inner_call_when_wrapped() {
    let left = rust("fn main() { foo(bar(123)); }");
    let right = rust("fn main() { foo(extra(bar(123))); }");
    let diff = compare_trees_graph("rust", &left, &right, 100);
    assert_eq!(diff.removed_nodes, 0, "{:?}", diff.chunks);
    assert_eq!(diff.modified_nodes, 0, "{:?}", diff.chunks);
    assert_eq!(diff.moved_nodes, 0, "{:?}", diff.chunks);
    assert!(
        diff.chunks
            .iter()
            .any(|chunk| chunk.tag == TreeTag::Added && chunk.text.contains("extra")),
        "{:?}",
        diff.chunks
    );
}

#[test]
fn graph_mode_keeps_inner_token_when_wrapped() {
    let left = rust("fn main() { let x = y; }");
    let right = rust("fn main() { let x = (y); }");
    let diff = compare_trees_graph("rust", &left, &right, 100);
    assert_eq!(diff.removed_nodes, 0, "{:?}", diff.chunks);
    assert_eq!(diff.modified_nodes, 0, "{:?}", diff.chunks);
    assert_eq!(diff.moved_nodes, 0, "{:?}", diff.chunks);
    let added: String = diff
        .chunks
        .iter()
        .filter(|chunk| chunk.tag == TreeTag::Added)
        .map(|chunk| chunk.text.as_str())
        .collect();
    assert_eq!(added, "()", "{:?}", diff.chunks);
}

#[test]
fn graph_mode_highlights_changed_delimiters() {
    let left = rust("fn main() { let x = (y); }");
    let right = rust("fn main() { let x = [y]; }");
    let diff = compare_trees_graph("rust", &left, &right, 100);
    let removed: String = diff
        .chunks
        .iter()
        .filter(|chunk| chunk.tag == TreeTag::Removed)
        .map(|chunk| chunk.text.as_str())
        .collect();
    let added: String = diff
        .chunks
        .iter()
        .filter(|chunk| chunk.tag == TreeTag::Added)
        .map(|chunk| chunk.text.as_str())
        .collect();
    assert!(
        removed.contains('(') && removed.contains(')'),
        "{removed:?}"
    );
    assert!(added.contains('[') && added.contains(']'), "{added:?}");
    assert!(
        !diff
            .chunks
            .iter()
            .any(|chunk| chunk.text == "y" && chunk.tag != TreeTag::Moved),
        "{:?}",
        diff.chunks
    );
}

#[test]
fn graph_mode_keeps_existing_call_before_inserted_argument() {
    let left = rust("fn main() { foo(bar()); }");
    let right = rust("fn main() { foo(novel(), bar()); }");
    let diff = compare_trees_graph("rust", &left, &right, 100);
    assert_eq!(diff.removed_nodes, 0, "{:?}", diff.chunks);
    assert_eq!(diff.modified_nodes, 0, "{:?}", diff.chunks);
    assert_eq!(diff.moved_nodes, 0, "{:?}", diff.chunks);
    assert!(
        diff.chunks
            .iter()
            .any(|chunk| chunk.tag == TreeTag::Added && chunk.text.contains("novel")),
        "{:?}",
        diff.chunks
    );
    assert!(
        !diff
            .chunks
            .iter()
            .any(|chunk| chunk.text.contains("bar ( )")),
        "{:?}",
        diff.chunks
    );
}

#[test]
fn graph_mode_handles_nested_repeated_calls() {
    let left = rust("fn main() { foo(bar(1), bar(2)); }");
    let right = rust("fn main() { foo(bar(2), bar(1)); }");
    let diff = compare_trees_graph("rust", &left, &right, 100);
    assert!(diff.moved_nodes > 0, "{:?}", diff);
    assert_eq!(diff.added_nodes, 0, "{:?}", diff);
    assert_eq!(diff.removed_nodes, 0, "{:?}", diff);
    assert_eq!(diff.modified_nodes, 0, "{:?}", diff);
    assert!(
        diff.chunks
            .iter()
            .any(|chunk| chunk.tag == TreeTag::Moved && chunk.text.contains("bar ( 2 )")),
        "{:?}",
        diff.chunks
    );
}

#[test]
fn graph_mode_moves_nested_block() {
    let left = rust("fn main() { if cond { foo(); } bar(); }");
    let right = rust("fn main() { bar(); if cond { foo(); } }");
    let diff = compare_trees_graph("rust", &left, &right, 100);
    assert!(diff.moved_nodes > 0, "{:?}", diff);
    assert_eq!(diff.added_nodes, 0, "{:?}", diff);
    assert_eq!(diff.removed_nodes, 0, "{:?}", diff);
    assert_eq!(diff.modified_nodes, 0, "{:?}", diff);
    assert!(
        diff.chunks
            .iter()
            .any(|chunk| chunk.tag == TreeTag::Moved && chunk.text.contains("bar ( ) ;")),
        "{:?}",
        diff.chunks
    );
}

#[test]
fn signature_lcs_prefers_minimal_index_shift() {
    let left = vec!["a".to_string(), "a".to_string(), "a".to_string()];
    let right = vec!["a".to_string(), "a".to_string()];
    assert_eq!(signature_lcs_pairs(&left, &right), vec![(0, 0), (1, 1)]);
}

#[test]
fn graph_mode_slides_duplicate_statements() {
    let left = rust("fn main() { foo(); foo(); bar(); }");
    let right = rust("fn main() { bar(); foo(); foo(); }");
    let diff = compare_trees_graph("rust", &left, &right, 100);
    let moved: Vec<_> = diff
        .chunks
        .iter()
        .filter(|chunk| chunk.tag == TreeTag::Moved)
        .collect();
    assert_eq!(moved.len(), 1, "{:?}", diff.chunks);
    assert!(moved[0].text.contains("bar"), "{:?}", moved[0]);
}

#[test]
fn optimal_child_matching_beats_greedy() {
    let scores = vec![vec![10, 9], vec![9, 0]];
    let greedy = greedy_child_matches(&scores);
    let optimal = optimal_child_matches(&scores);
    assert_eq!(greedy, vec![Some(0), None], "{greedy:?}");
    assert_eq!(optimal, vec![Some(1), Some(0)], "{optimal:?}");
}

#[test]
fn aligns_identical_siblings_by_content() {
    let left = rust("fn main() { let a = 1; let b = 2; }");
    let right = rust("fn main() { let b = 2; let x = 9; }");
    let diff = compare_trees("rust", &left, &right, 100);
    assert!(diff.added_nodes > 0, "{:?}", diff);
    assert!(diff.removed_nodes > 0, "{:?}", diff);
    assert_eq!(diff.modified_nodes, 0, "{:?}", diff);
}

#[test]
fn comments_can_be_ignored() {
    let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    let options = TreeOptions {
        ignore_comments: true,
        ..TreeOptions::default()
    };
    let left = build_tree("fn main() { /* old */ let x = 1; }", &language, options).unwrap();
    let right = build_tree("fn main() { /* new */ let x = 1; }", &language, options).unwrap();
    let diff = compare_trees("rust", &left, &right, 100);
    assert!(diff.chunks.is_empty());
}
