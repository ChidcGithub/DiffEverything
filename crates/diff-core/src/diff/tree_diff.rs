use crate::model::TreeTag;
use similar::{Algorithm, DiffOp, capture_diff_slices};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct SyntaxNode {
    pub kind: String,
    pub text: String,
    pub children: Vec<SyntaxNode>,
}

impl SyntaxNode {
    fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    fn node_count(&self) -> usize {
        1 + self
            .children
            .iter()
            .map(SyntaxNode::node_count)
            .sum::<usize>()
    }
}

#[derive(Debug, Clone)]
pub struct TreeDiffResult {
    pub language: String,
    pub added_nodes: usize,
    pub removed_nodes: usize,
    pub modified_nodes: usize,
    pub moved_nodes: usize,
    pub chunks: Vec<TreeChunkResult>,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub struct TreeChunkResult {
    pub tag: TreeTag,
    pub path: String,
    pub text: String,
    pub depth: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct TreeOptions {
    pub ignore_comments: bool,
    pub max_nodes: usize,
    pub max_depth: usize,
}

impl Default for TreeOptions {
    fn default() -> Self {
        Self {
            ignore_comments: false,
            max_nodes: 50_000,
            max_depth: 64,
        }
    }
}

struct BuildState {
    count: usize,
    options: TreeOptions,
}

pub fn build_tree(
    source: &str,
    language: &tree_sitter::Language,
    options: TreeOptions,
) -> std::result::Result<SyntaxNode, String> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(language)
        .map_err(|error| error.to_string())?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "tree-sitter returned no tree".to_string())?;
    let mut state = BuildState { count: 0, options };
    build_node(tree.root_node(), source.as_bytes(), 0, &mut state)?
        .ok_or_else(|| "empty syntax tree".to_string())
}

fn build_node(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    depth: usize,
    state: &mut BuildState,
) -> std::result::Result<Option<SyntaxNode>, String> {
    if state.options.ignore_comments && node.kind().contains("comment") {
        return Ok(None);
    }
    state.count += 1;
    if state.count > state.options.max_nodes {
        return Err(format!(
            "syntax node limit exceeded (max {})",
            state.options.max_nodes
        ));
    }

    let kind = node.kind().to_string();
    let has_children = node.child_count() > 0 && depth < state.options.max_depth;
    let mut children = Vec::new();
    if has_children {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(child) = build_node(child, source, depth + 1, state)? {
                children.push(child);
            }
        }
    }

    let text = if has_children {
        String::new()
    } else {
        let text = node.utf8_text(source).unwrap_or("").trim();
        truncate(text, 500)
    };

    Ok(Some(SyntaxNode {
        kind,
        text,
        children,
    }))
}

pub fn compare_trees(
    language: &str,
    left: &SyntaxNode,
    right: &SyntaxNode,
    max_chunks: usize,
) -> TreeDiffResult {
    let mut result = TreeDiffResult {
        language: language.to_string(),
        added_nodes: 0,
        removed_nodes: 0,
        modified_nodes: 0,
        moved_nodes: 0,
        chunks: Vec::new(),
        truncated: false,
    };
    diff_node(left, right, &left.kind, 0, max_chunks, &mut result);
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GraphEventKind {
    Enter,
    Token,
    Exit,
}

#[derive(Debug, Clone)]
struct GraphEvent {
    kind: GraphEventKind,
    node_kind: String,
    text: String,
    path: String,
    depth: usize,
    node_count: usize,
}

#[derive(Debug, Clone)]
struct GraphMove {
    left_path: String,
    right_path: String,
    text: String,
    depth: usize,
    node_count: usize,
    exact: bool,
}

type JumpTargets = BTreeMap<(usize, usize), Vec<(usize, usize, i32)>>;

/// Linearize both syntax trees into enter/token/exit events and run a bounded
/// shortest-path (dynamic programming) alignment over the two event graphs.
/// This is a simplified difftastic-style graph diff for the CLI.
pub fn compare_trees_graph(
    language: &str,
    left: &SyntaxNode,
    right: &SyntaxNode,
    max_chunks: usize,
) -> TreeDiffResult {
    let mut left_events = Vec::new();
    let mut right_events = Vec::new();
    collect_graph_events(left, &left.kind, 0, &mut left_events);
    collect_graph_events(right, &right.kind, 0, &mut right_events);
    if left_events.is_empty()
        || right_events.is_empty()
        || left_events.len() > 4_000
        || right_events.len() > 4_000
        || left_events.len().saturating_mul(right_events.len()) > 4_000_000
    {
        return compare_trees(language, left, right, max_chunks);
    }

    let rows = left_events.len() + 1;
    let columns = right_events.len() + 1;
    let left_exits = graph_matching_exits(&left_events);
    let right_exits = graph_matching_exits(&right_events);
    let moves = {
        let mut stable = Vec::new();
        let moves = collect_graph_moves(left, right, &left.kind, &right.kind, &mut stable);
        (moves, stable)
    };
    let (moves, stable) = moves;

    // Exact moved subtrees become non-local zero-cost edges in the alignment
    // graph. This lets the shortest path jump over a moved block instead of
    // paying delete+insert costs and relying on post-hoc suppression.
    let mut jump_targets: JumpTargets = BTreeMap::new();
    for moved in &moves {
        if !moved.exact {
            continue;
        }
        let left_start = left_events
            .iter()
            .position(|event| event.kind == GraphEventKind::Enter && event.path == moved.left_path);
        let right_start = right_events.iter().position(|event| {
            event.kind == GraphEventKind::Enter && event.path == moved.right_path
        });
        if let (Some(left_start), Some(right_start)) = (left_start, right_start)
            && let (Some(left_end), Some(right_end)) =
                (left_exits[left_start], right_exits[right_start])
        {
            jump_targets
                .entry((left_end + 1, right_end + 1))
                .or_default()
                .push((left_start, right_start, 0));
        }
    }

    for (left_path, right_path) in &stable {
        let left_index = left_events.iter().position(|event| {
            event.path == *left_path
                && matches!(event.kind, GraphEventKind::Enter | GraphEventKind::Token)
        });
        let right_index = right_events.iter().position(|event| {
            event.path == *right_path
                && matches!(event.kind, GraphEventKind::Enter | GraphEventKind::Token)
        });
        let (Some(left_index), Some(right_index)) = (left_index, right_index) else {
            continue;
        };
        if let (GraphEventKind::Enter, GraphEventKind::Enter) =
            (left_events[left_index].kind, right_events[right_index].kind)
            && let (Some(left_end), Some(right_end)) =
                (left_exits[left_index], right_exits[right_index])
        {
            jump_targets
                .entry((left_end + 1, right_end + 1))
                .or_default()
                .push((left_index, right_index, 0));
        }
    }

    const INF: i32 = 1_000_000_000;
    let mut costs = vec![vec![INF; columns]; rows];
    costs[0][0] = 0;
    for (row, cost_row) in costs.iter_mut().enumerate().skip(1) {
        cost_row[0] = row as i32;
    }
    for (column, cost) in costs[0].iter_mut().enumerate().skip(1) {
        *cost = column as i32;
    }
    for row in 1..rows {
        for column in 1..columns {
            let mut best = costs[row][column];
            if let Some(starts) = jump_targets.get(&(row, column)) {
                for (start_row, start_column, cost_delta) in starts {
                    best = best.min(costs[*start_row][*start_column] + *cost_delta);
                }
            }
            let substitution = graph_event_cost(&left_events[row - 1], &right_events[column - 1]);
            best = best
                .min(costs[row - 1][column - 1] + substitution)
                .min(costs[row - 1][column] + 1)
                .min(costs[row][column - 1] + 1);
            costs[row][column] = best;
        }
    }

    let mut operations = Vec::new();
    let mut row = rows - 1;
    let mut column = columns - 1;
    while row > 0 || column > 0 {
        if let Some(starts) = jump_targets.get(&(row, column))
            && let Some((start_row, start_column, _)) =
                starts.iter().find(|(start_row, start_column, cost_delta)| {
                    costs[*start_row][*start_column] + *cost_delta == costs[row][column]
                })
        {
            row = *start_row;
            column = *start_column;
            continue;
        }
        if row > 0 && column > 0 {
            let substitution = graph_event_cost(&left_events[row - 1], &right_events[column - 1]);
            if costs[row][column] == costs[row - 1][column - 1] + substitution {
                operations.push((Some(row - 1), Some(column - 1)));
                row -= 1;
                column -= 1;
                continue;
            }
        }
        if row > 0 && costs[row][column] == costs[row - 1][column] + 1 {
            operations.push((Some(row - 1), None));
            row -= 1;
        } else {
            operations.push((None, Some(column - 1)));
            column -= 1;
        }
    }
    operations.reverse();

    let mut deleted = vec![false; left_events.len()];
    let mut inserted = vec![false; right_events.len()];
    for (left_index, right_index) in &operations {
        if right_index.is_none()
            && let Some(left_index) = left_index
        {
            deleted[*left_index] = true;
        }
        if left_index.is_none()
            && let Some(right_index) = right_index
        {
            inserted[*right_index] = true;
        }
    }
    let left_prefix = graph_prefix_sums(&deleted);
    let right_prefix = graph_prefix_sums(&inserted);
    let mut left_partner = vec![None; left_events.len()];
    let mut right_partner = vec![None; right_events.len()];
    for (left_index, right_index) in &operations {
        if let (Some(left_index), Some(right_index)) = (left_index, right_index) {
            let left_event = &left_events[*left_index];
            let right_event = &right_events[*right_index];
            if left_event.kind == GraphEventKind::Token
                && right_event.kind == GraphEventKind::Token
                && left_event.text != right_event.text
            {
                left_partner[*left_index] = Some(*right_index);
                right_partner[*right_index] = Some(*left_index);
            }
        }
    }
    let mut consumed_left = vec![false; left_events.len()];
    let mut consumed_right = vec![false; right_events.len()];

    let mut result = TreeDiffResult {
        language: language.to_string(),
        added_nodes: 0,
        removed_nodes: 0,
        modified_nodes: 0,
        moved_nodes: 0,
        chunks: Vec::new(),
        truncated: false,
    };
    let mut seen = std::collections::BTreeSet::new();
    for (left_index, right_index) in operations {
        if let Some(left_index) = left_index
            && consumed_left[left_index]
        {
            continue;
        }
        if let Some(right_index) = right_index
            && consumed_right[right_index]
        {
            continue;
        }
        let exact_move = left_index.is_some_and(|left_index| {
            moves
                .iter()
                .any(|m| m.exact && path_under(&left_events[left_index].path, &m.left_path))
        }) || right_index.is_some_and(|right_index| {
            moves
                .iter()
                .any(|m| m.exact && path_under(&right_events[right_index].path, &m.right_path))
        });
        if exact_move {
            continue;
        }
        let stable_side = left_index.is_some_and(|left_index| {
            stable
                .iter()
                .any(|(path, _)| path_under(&left_events[left_index].path, path))
        }) || right_index.is_some_and(|right_index| {
            stable
                .iter()
                .any(|(_, path)| path_under(&right_events[right_index].path, path))
        });
        if stable_side {
            continue;
        }
        let (tag, path, text, depth, node_count) = match (left_index, right_index) {
            (Some(left_index), Some(right_index)) => {
                let left_event = &left_events[left_index];
                let right_event = &right_events[right_index];
                if left_event.kind == GraphEventKind::Enter
                    && right_event.kind == GraphEventKind::Enter
                    && left_event.path == right_event.path
                    && let Some(left_end) = left_exits[left_index]
                    && let Some(right_end) = right_exits[right_index]
                    && graph_block_modified(
                        &left_events,
                        &right_events,
                        &left_partner,
                        &right_partner,
                        &left_prefix,
                        &right_prefix,
                        left_index,
                        left_end,
                        right_index,
                        right_end,
                    )
                {
                    for consumed in &mut consumed_left[left_index..=left_end] {
                        *consumed = true;
                    }
                    for consumed in &mut consumed_right[right_index..=right_end] {
                        *consumed = true;
                    }
                    (
                        TreeTag::Modified,
                        left_event.path.clone(),
                        format!(
                            "{} -> {}",
                            graph_subtree_text(left_event),
                            graph_subtree_text(right_event)
                        ),
                        left_event.depth,
                        left_event.node_count,
                    )
                } else if left_event.kind == GraphEventKind::Token
                    && right_event.kind == GraphEventKind::Token
                    && left_event.text != right_event.text
                {
                    (
                        TreeTag::Modified,
                        left_event.path.clone(),
                        format!(
                            "{} -> {}",
                            truncate(&left_event.text, 200),
                            truncate(&right_event.text, 200)
                        ),
                        left_event.depth,
                        1,
                    )
                } else {
                    continue;
                }
            }
            (Some(left_index), None) => {
                let event = &left_events[left_index];
                if event.kind == GraphEventKind::Enter
                    && let Some(end) = left_exits[left_index]
                    && graph_fully_changed(&left_prefix, left_index, end)
                {
                    for consumed in &mut consumed_left[left_index..=end] {
                        *consumed = true;
                    }
                    (
                        TreeTag::Removed,
                        event.path.clone(),
                        graph_subtree_text(event),
                        event.depth,
                        event.node_count,
                    )
                } else if event.kind == GraphEventKind::Token {
                    (
                        TreeTag::Removed,
                        event.path.clone(),
                        truncate(&event.text, 200),
                        event.depth,
                        1,
                    )
                } else {
                    continue;
                }
            }
            (None, Some(right_index)) => {
                let event = &right_events[right_index];
                if event.kind == GraphEventKind::Enter
                    && let Some(end) = right_exits[right_index]
                    && graph_fully_changed(&right_prefix, right_index, end)
                {
                    for consumed in &mut consumed_right[right_index..=end] {
                        *consumed = true;
                    }
                    (
                        TreeTag::Added,
                        event.path.clone(),
                        graph_subtree_text(event),
                        event.depth,
                        event.node_count,
                    )
                } else if event.kind == GraphEventKind::Token {
                    (
                        TreeTag::Added,
                        event.path.clone(),
                        truncate(&event.text, 200),
                        event.depth,
                        1,
                    )
                } else {
                    continue;
                }
            }
            (None, None) => continue,
        };
        let key = (tag as u8, path.clone());
        if !seen.insert(key) {
            continue;
        }
        if result.chunks.len() >= max_chunks {
            result.truncated = true;
            break;
        }
        match tag {
            TreeTag::Added => result.added_nodes += node_count,
            TreeTag::Removed => result.removed_nodes += node_count,
            TreeTag::Modified => result.modified_nodes += node_count,
            TreeTag::Moved => result.moved_nodes += node_count,
        }
        result.chunks.push(TreeChunkResult {
            tag,
            path,
            text,
            depth,
        });
    }
    for moved in moves {
        if result.chunks.len() >= max_chunks {
            result.truncated = true;
            break;
        }
        result.moved_nodes += moved.node_count;
        result.chunks.push(TreeChunkResult {
            tag: TreeTag::Moved,
            path: moved.left_path,
            text: moved.text,
            depth: moved.depth,
        });
    }
    result
}

fn graph_matching_exits(events: &[GraphEvent]) -> Vec<Option<usize>> {
    let mut matches = vec![None; events.len()];
    let mut stack = Vec::new();
    for (index, event) in events.iter().enumerate() {
        match event.kind {
            GraphEventKind::Enter => stack.push(index),
            GraphEventKind::Exit => {
                if let Some(start) = stack.pop() {
                    matches[start] = Some(index);
                    matches[index] = Some(start);
                }
            }
            GraphEventKind::Token => {}
        }
    }
    matches
}

fn graph_prefix_sums(flags: &[bool]) -> Vec<usize> {
    let mut prefix = vec![0usize; flags.len() + 1];
    for (index, flag) in flags.iter().enumerate() {
        prefix[index + 1] = prefix[index] + usize::from(*flag);
    }
    prefix
}

fn graph_fully_changed(prefix: &[usize], start: usize, end: usize) -> bool {
    prefix[end + 1] - prefix[start] == end - start + 1
}

#[allow(clippy::too_many_arguments)]
fn graph_block_modified(
    left_events: &[GraphEvent],
    right_events: &[GraphEvent],
    left_partner: &[Option<usize>],
    right_partner: &[Option<usize>],
    left_prefix: &[usize],
    right_prefix: &[usize],
    left_start: usize,
    left_end: usize,
    right_start: usize,
    right_end: usize,
) -> bool {
    if left_prefix[left_end + 1] - left_prefix[left_start] != 0
        || right_prefix[right_end + 1] - right_prefix[right_start] != 0
    {
        return false;
    }
    let mut token_count = 0usize;
    let mut modified_count = 0usize;
    for index in left_start..=left_end {
        if left_events[index].kind != GraphEventKind::Token {
            continue;
        }
        token_count += 1;
        if let Some(partner) = left_partner[index] {
            if partner < right_start || partner > right_end {
                return false;
            }
            modified_count += 1;
        }
    }
    for index in right_start..=right_end {
        if right_events[index].kind != GraphEventKind::Token {
            continue;
        }
        if let Some(partner) = right_partner[index]
            && (partner < left_start || partner > left_end)
        {
            return false;
        }
    }
    modified_count > 0 && modified_count * 2 >= token_count
}

fn graph_subtree_text(event: &GraphEvent) -> String {
    if event.text.is_empty() {
        format!("<{}>", event.node_kind)
    } else {
        truncate(&event.text, 500)
    }
}

fn path_under(path: &str, parent: &str) -> bool {
    path == parent || (path.starts_with(parent) && path.as_bytes().get(parent.len()) == Some(&b'/'))
}

fn signature_lcs_pairs(left: &[String], right: &[String]) -> Vec<(usize, usize)> {
    let mut lengths = vec![vec![0usize; right.len() + 1]; left.len() + 1];
    let mut costs = vec![vec![0usize; right.len() + 1]; left.len() + 1];
    for left_index in 1..=left.len() {
        for right_index in 1..=right.len() {
            let up = (
                lengths[left_index - 1][right_index],
                costs[left_index - 1][right_index],
            );
            let side = (
                lengths[left_index][right_index - 1],
                costs[left_index][right_index - 1],
            );
            let mut best = if up.0 > side.0 || (up.0 == side.0 && up.1 <= side.1) {
                up
            } else {
                side
            };
            if left[left_index - 1] == right[right_index - 1] {
                let diagonal = (
                    lengths[left_index - 1][right_index - 1] + 1,
                    costs[left_index - 1][right_index - 1] + left_index.abs_diff(right_index),
                );
                if diagonal.0 > best.0 || (diagonal.0 == best.0 && diagonal.1 < best.1) {
                    best = diagonal;
                }
            }
            lengths[left_index][right_index] = best.0;
            costs[left_index][right_index] = best.1;
        }
    }
    let mut pairs = Vec::new();
    let mut left_index = left.len();
    let mut right_index = right.len();
    while left_index > 0 && right_index > 0 {
        if left[left_index - 1] == right[right_index - 1]
            && lengths[left_index][right_index] == lengths[left_index - 1][right_index - 1] + 1
            && costs[left_index][right_index]
                == costs[left_index - 1][right_index - 1] + left_index.abs_diff(right_index)
        {
            pairs.push((left_index - 1, right_index - 1));
            left_index -= 1;
            right_index -= 1;
        } else if lengths[left_index - 1][right_index] > lengths[left_index][right_index - 1]
            || (lengths[left_index - 1][right_index] == lengths[left_index][right_index - 1]
                && costs[left_index - 1][right_index] <= costs[left_index][right_index - 1])
        {
            left_index -= 1;
        } else {
            right_index -= 1;
        }
    }
    pairs.reverse();
    pairs
}

fn collect_graph_moves(
    left: &SyntaxNode,
    right: &SyntaxNode,
    left_path: &str,
    right_path: &str,
    stable: &mut Vec<(String, String)>,
) -> Vec<GraphMove> {
    let mut moves = Vec::new();
    collect_graph_moves_at(left, right, left_path, right_path, 0, &mut moves, stable);
    moves
}

fn collect_graph_moves_at(
    left: &SyntaxNode,
    right: &SyntaxNode,
    left_path: &str,
    right_path: &str,
    depth: usize,
    moves: &mut Vec<GraphMove>,
    stable: &mut Vec<(String, String)>,
) {
    if moves.len() >= 100 || left.children.is_empty() || right.children.is_empty() {
        return;
    }
    let left_signatures: Vec<String> = left.children.iter().map(node_signature).collect();
    let right_signatures: Vec<String> = right.children.iter().map(node_signature).collect();
    let mut left_used = vec![false; left.children.len()];
    let mut right_used = vec![false; right.children.len()];
    for (left_index, right_index) in signature_lcs_pairs(&left_signatures, &right_signatures) {
        left_used[left_index] = true;
        right_used[right_index] = true;
        let left_child = &left.children[left_index];
        let right_child = &right.children[right_index];
        let left_child_path = format!("{left_path}/{}[{}]", left_child.kind, left_index);
        let right_child_path = format!("{right_path}/{}[{}]", right_child.kind, right_index);
        let signature = &left_signatures[left_index];
        let unique = left_signatures
            .iter()
            .filter(|item| *item == signature)
            .count()
            == 1
            && right_signatures
                .iter()
                .filter(|item| *item == signature)
                .count()
                == 1;
        if unique {
            stable.push((left_child_path.clone(), right_child_path.clone()));
        }
        collect_graph_moves_at(
            left_child,
            right_child,
            &left_child_path,
            &right_child_path,
            depth + 1,
            moves,
            stable,
        );
    }
    let unmatched_left: Vec<usize> = (0..left.children.len())
        .filter(|index| !left_used[*index])
        .collect();
    let unmatched_right: Vec<usize> = (0..right.children.len())
        .filter(|index| !right_used[*index])
        .collect();

    let mut left_moved = vec![false; left.children.len()];
    for left_index in unmatched_left {
        let left_child = &left.children[left_index];
        let signature = &left_signatures[left_index];
        let Some(right_index) = unmatched_right.iter().copied().find(|right_index| {
            !right_used[*right_index] && right_signatures[*right_index] == *signature
        }) else {
            continue;
        };
        right_used[right_index] = true;
        left_moved[left_index] = true;
        let right_child = &right.children[right_index];
        let left_child_path = format!("{left_path}/{}[{}]", left_child.kind, left_index);
        let right_child_path = format!("{right_path}/{}[{}]", right_child.kind, right_index);
        if left_child_path == right_child_path {
            continue;
        }
        moves.push(GraphMove {
            left_path: left_child_path,
            right_path: right_child_path,
            text: truncate(&node_text(left_child), 200),
            depth,
            node_count: left_child.node_count(),
            exact: true,
        });
    }

    for left_index in 0..left.children.len() {
        if left_used[left_index] || left_moved[left_index] {
            continue;
        }
        let left_child = &left.children[left_index];
        let Some(right_index) = (0..right.children.len()).find(|right_index| {
            !right_used[*right_index] && right.children[*right_index].kind == left_child.kind
        }) else {
            continue;
        };
        right_used[right_index] = true;
        let right_child = &right.children[right_index];
        let left_child_path = format!("{left_path}/{}[{}]", left_child.kind, left_index);
        let right_child_path = format!("{right_path}/{}[{}]", right_child.kind, right_index);
        let already_moved = moves
            .iter()
            .any(|m| path_under(&left_child_path, &m.left_path));
        if !already_moved
            && left_child_path != right_child_path
            && node_similarity(left_child, right_child) >= 50
        {
            moves.push(GraphMove {
                left_path: left_child_path.clone(),
                right_path: right_child_path.clone(),
                text: truncate(&node_text(left_child), 200),
                depth,
                node_count: left_child.node_count(),
                exact: false,
            });
        }
        collect_graph_moves_at(
            left_child,
            right_child,
            &left_child_path,
            &right_child_path,
            depth + 1,
            moves,
            stable,
        );
    }
}

fn node_signature(node: &SyntaxNode) -> String {
    let mut signature = String::new();
    append_node_signature(node, &mut signature);
    signature
}

fn append_node_signature(node: &SyntaxNode, out: &mut String) {
    out.push_str(&node.kind);
    out.push('|');
    out.push_str(&node.text);
    out.push('|');
    for child in &node.children {
        append_node_signature(child, out);
        out.push(',');
    }
}

fn node_text(node: &SyntaxNode) -> String {
    let mut text = String::new();
    append_node_text(node, &mut text);
    text.trim().to_string()
}

fn node_similarity(left: &SyntaxNode, right: &SyntaxNode) -> usize {
    let left_tokens = leaf_tokens(left);
    let right_tokens = leaf_tokens(right);
    let max_len = left_tokens.len().max(right_tokens.len()).max(1);
    multiset_intersection(&left_tokens, &right_tokens) * 100 / max_len
}

fn append_node_text(node: &SyntaxNode, out: &mut String) {
    if node.children.is_empty() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(node.text.trim());
        return;
    }
    for child in &node.children {
        append_node_text(child, out);
    }
}

fn collect_graph_events(
    node: &SyntaxNode,
    path: &str,
    depth: usize,
    events: &mut Vec<GraphEvent>,
) -> (String, usize) {
    if node.children.is_empty() {
        let text = node.text.trim();
        if !text.is_empty() {
            events.push(GraphEvent {
                kind: GraphEventKind::Token,
                node_kind: node.kind.clone(),
                text: text.to_string(),
                path: path.to_string(),
                depth,
                node_count: 1,
            });
        }
        return (text.to_string(), 1);
    }
    let enter_index = events.len();
    events.push(GraphEvent {
        kind: GraphEventKind::Enter,
        node_kind: node.kind.clone(),
        text: String::new(),
        path: path.to_string(),
        depth,
        node_count: 0,
    });
    let mut subtree_text = String::new();
    let mut node_count = 1usize;
    for (index, child) in node.children.iter().enumerate() {
        let child_path = format!("{path}/{}[{}]", child.kind, index);
        let (child_text, child_count) = collect_graph_events(child, &child_path, depth + 1, events);
        if !child_text.is_empty() {
            if !subtree_text.is_empty() {
                subtree_text.push(' ');
            }
            subtree_text.push_str(&child_text);
        }
        node_count += child_count;
    }
    events[enter_index].text = truncate(&subtree_text, 500);
    events[enter_index].node_count = node_count;
    events.push(GraphEvent {
        kind: GraphEventKind::Exit,
        node_kind: node.kind.clone(),
        text: String::new(),
        path: path.to_string(),
        depth,
        node_count: 1,
    });
    (subtree_text, node_count)
}

fn graph_event_cost(left: &GraphEvent, right: &GraphEvent) -> i32 {
    match (left.kind, right.kind) {
        (GraphEventKind::Token, GraphEventKind::Token) => {
            if left.text == right.text {
                0
            } else if left.node_kind == right.node_kind {
                2
            } else {
                3
            }
        }
        (GraphEventKind::Enter, GraphEventKind::Enter)
        | (GraphEventKind::Exit, GraphEventKind::Exit) => {
            if left.node_kind == right.node_kind {
                if left.path == right.path { 0 } else { 2 }
            } else {
                4
            }
        }
        _ => 5,
    }
}

fn diff_node(
    left: &SyntaxNode,
    right: &SyntaxNode,
    path: &str,
    depth: usize,
    max_chunks: usize,
    result: &mut TreeDiffResult,
) {
    if nodes_equal(left, right) {
        return;
    }

    if left.kind != right.kind || left.is_leaf() || right.is_leaf() {
        if left.kind == right.kind && left.is_leaf() && right.is_leaf() {
            result.modified_nodes += 1;
            push_chunk(
                result,
                max_chunks,
                TreeChunkResult {
                    tag: TreeTag::Modified,
                    path: path.to_string(),
                    text: format!("{} -> {}", left.text, right.text),
                    depth,
                },
            );
        } else {
            push_subtree(result, max_chunks, left, path, depth, TreeTag::Removed);
            push_subtree(result, max_chunks, right, path, depth, TreeTag::Added);
        }
        return;
    }

    for (left_index, right_index) in align_children(&left.children, &right.children) {
        match (left_index, right_index) {
            (Some(left_index), Some(right_index)) => {
                let left_child = &left.children[left_index];
                let right_child = &right.children[right_index];
                let child_path = format!("{path}/{}[{}]", left_child.kind, left_index);
                diff_node(
                    left_child,
                    right_child,
                    &child_path,
                    depth + 1,
                    max_chunks,
                    result,
                );
            }
            (Some(left_index), None) => {
                let child = &left.children[left_index];
                let child_path = format!("{path}/{}[{}]", child.kind, left_index);
                push_subtree(
                    result,
                    max_chunks,
                    child,
                    &child_path,
                    depth + 1,
                    TreeTag::Removed,
                );
            }
            (None, Some(right_index)) => {
                let child = &right.children[right_index];
                let child_path = format!("{path}/{}[{}]", child.kind, right_index);
                push_subtree(
                    result,
                    max_chunks,
                    child,
                    &child_path,
                    depth + 1,
                    TreeTag::Added,
                );
            }
            (None, None) => {}
        }
    }
}

fn align_children(
    left: &[SyntaxNode],
    right: &[SyntaxNode],
) -> Vec<(Option<usize>, Option<usize>)> {
    if left.is_empty() {
        return (0..right.len()).map(|index| (None, Some(index))).collect();
    }
    if right.is_empty() {
        return (0..left.len()).map(|index| (Some(index), None)).collect();
    }
    if left.len().saturating_mul(right.len()) > 20_000 {
        return align_children_myers(left, right);
    }

    let right_tokens: Vec<Vec<String>> = right.iter().map(meaningful_leaf_tokens).collect();
    let mut scores = vec![vec![-10_000i32; right.len()]; left.len()];
    for (left_index, left_node) in left.iter().enumerate() {
        let left_tokens = meaningful_leaf_tokens(left_node);
        for (right_index, right_node) in right.iter().enumerate() {
            scores[left_index][right_index] = node_match_score_with_tokens(
                left_node,
                right_node,
                &left_tokens,
                &right_tokens[right_index],
            );
        }
    }

    let left_matches = if left.len() + right.len() <= 128 {
        let mut matches = optimal_child_matches(&scores);
        slide_ordered_matches(&scores, &mut matches);
        matches
    } else {
        greedy_child_matches(&scores)
    };
    let mut right_used = vec![false; right.len()];
    for right_index in left_matches.iter().flatten() {
        right_used[*right_index] = true;
    }

    let mut pairs = Vec::new();
    for (left_index, right_index) in left_matches.into_iter().enumerate() {
        pairs.push((Some(left_index), right_index));
    }
    for (right_index, used) in right_used.into_iter().enumerate() {
        if !used {
            pairs.push((None, Some(right_index)));
        }
    }
    pairs
}

fn greedy_child_matches(scores: &[Vec<i32>]) -> Vec<Option<usize>> {
    let mut candidates = Vec::new();
    for (left_index, row) in scores.iter().enumerate() {
        for (right_index, score) in row.iter().enumerate() {
            if *score >= 5 {
                candidates.push((*score, left_index, right_index));
            }
        }
    }
    candidates.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then(left.1.cmp(&right.1))
            .then(left.2.cmp(&right.2))
    });
    let mut left_matches = vec![None; scores.len()];
    let mut right_used = vec![false; scores.first().map(Vec::len).unwrap_or(0)];
    for (_, left_index, right_index) in candidates {
        if left_matches[left_index].is_some() || right_used[right_index] {
            continue;
        }
        left_matches[left_index] = Some(right_index);
        right_used[right_index] = true;
    }
    left_matches
}

/// Maximum-weight bipartite matching. Real nodes may stay unmatched via
/// dummy rows/columns; incompatible pairs have zero weight. This replaces
/// the previous greedy pairing for sibling lists of moderate size.
fn optimal_child_matches(scores: &[Vec<i32>]) -> Vec<Option<usize>> {
    let left_len = scores.len();
    let right_len = scores.first().map(Vec::len).unwrap_or(0);
    let size = left_len + right_len;
    let mut cost = vec![vec![0i64; size]; size];
    for (left_index, row) in scores.iter().enumerate() {
        for (right_index, score) in row.iter().enumerate() {
            if *score >= 5 {
                cost[left_index][right_index] = -i64::from(*score);
            }
        }
    }
    let assignment = hungarian_assignment(&cost);
    let mut left_matches = vec![None; left_len];
    for (left_index, right_index) in assignment.into_iter().enumerate() {
        if left_index < left_len && right_index < right_len && scores[left_index][right_index] >= 5
        {
            left_matches[left_index] = Some(right_index);
        }
    }
    left_matches
}

fn slide_ordered_matches(scores: &[Vec<i32>], matches: &mut [Option<usize>]) {
    let mut changed = true;
    while changed {
        changed = false;
        for left1 in 0..matches.len() {
            for left2 in left1 + 1..matches.len() {
                let (Some(right1), Some(right2)) = (matches[left1], matches[left2]) else {
                    continue;
                };
                if right1 <= right2 {
                    continue;
                }
                if scores[left1][right2] < 5 || scores[left2][right1] < 5 {
                    continue;
                }
                if scores[left1][right1] + scores[left2][right2]
                    == scores[left1][right2] + scores[left2][right1]
                {
                    matches[left1] = Some(right2);
                    matches[left2] = Some(right1);
                    changed = true;
                }
            }
        }
    }
}

fn hungarian_assignment(cost: &[Vec<i64>]) -> Vec<usize> {
    let size = cost.len();
    if size == 0 {
        return Vec::new();
    }
    let mut u = vec![0i64; size + 1];
    let mut v = vec![0i64; size + 1];
    let mut p = vec![0usize; size + 1];
    let mut way = vec![0usize; size + 1];
    for row in 1..=size {
        p[0] = row;
        let mut column0 = 0usize;
        let mut minv = vec![i64::MAX; size + 1];
        let mut used = vec![false; size + 1];
        loop {
            used[column0] = true;
            let row0 = p[column0];
            let mut delta = i64::MAX;
            let mut column1 = 0usize;
            for column in 1..=size {
                if used[column] {
                    continue;
                }
                let current = cost[row0 - 1][column - 1] - u[row0] - v[column];
                if current < minv[column] {
                    minv[column] = current;
                    way[column] = column0;
                }
                if minv[column] < delta {
                    delta = minv[column];
                    column1 = column;
                }
            }
            for column in 0..=size {
                if used[column] {
                    u[p[column]] += delta;
                    v[column] -= delta;
                } else {
                    minv[column] -= delta;
                }
            }
            column0 = column1;
            if p[column0] == 0 {
                break;
            }
        }
        loop {
            let column1 = way[column0];
            p[column0] = p[column1];
            column0 = column1;
            if column0 == 0 {
                break;
            }
        }
    }
    let mut assignment = vec![usize::MAX; size];
    for column in 1..=size {
        if p[column] != 0 {
            assignment[p[column] - 1] = column - 1;
        }
    }
    assignment
}

fn node_match_score_with_tokens(
    left: &SyntaxNode,
    right: &SyntaxNode,
    left_tokens: &[String],
    right_tokens: &[String],
) -> i32 {
    if left.kind != right.kind {
        return -10_000;
    }
    if signature(left) == signature(right) {
        return 12;
    }
    if left.is_leaf() && right.is_leaf() {
        return 5;
    }
    let max_len = left_tokens.len().max(right_tokens.len());
    if max_len == 0 {
        return 2;
    }
    let common = multiset_intersection(left_tokens, right_tokens);
    2 + (common * 8 / max_len) as i32
}

fn meaningful_leaf_tokens(node: &SyntaxNode) -> Vec<String> {
    leaf_tokens(node)
        .into_iter()
        .filter(|token| {
            token.split_once(':').is_some_and(|(_, text)| {
                text.chars()
                    .any(|character| character.is_alphanumeric() || character == '_')
            })
        })
        .collect()
}

fn align_children_myers(
    left: &[SyntaxNode],
    right: &[SyntaxNode],
) -> Vec<(Option<usize>, Option<usize>)> {
    let left_signatures: Vec<String> = left.iter().map(signature).collect();
    let right_signatures: Vec<String> = right.iter().map(signature).collect();
    let ops = capture_diff_slices(Algorithm::Myers, &left_signatures, &right_signatures);
    let mut pairs = Vec::new();
    for op in ops {
        match op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => {
                for offset in 0..len {
                    pairs.push((Some(old_index + offset), Some(new_index + offset)));
                }
            }
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                for offset in 0..old_len {
                    pairs.push((Some(old_index + offset), None));
                }
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                for offset in 0..new_len {
                    pairs.push((None, Some(new_index + offset)));
                }
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                let paired = old_len.min(new_len);
                for offset in 0..paired {
                    pairs.push((Some(old_index + offset), Some(new_index + offset)));
                }
                for offset in paired..old_len {
                    pairs.push((Some(old_index + offset), None));
                }
                for offset in paired..new_len {
                    pairs.push((None, Some(new_index + offset)));
                }
            }
        }
    }
    pairs
}

fn leaf_tokens(node: &SyntaxNode) -> Vec<String> {
    let mut tokens = Vec::new();
    collect_leaf_tokens(node, &mut tokens);
    tokens.sort();
    tokens
}

fn collect_leaf_tokens(node: &SyntaxNode, tokens: &mut Vec<String>) {
    if node.is_leaf() {
        if !node.text.is_empty() {
            tokens.push(format!("{}:{}", node.kind, node.text));
        }
        return;
    }
    for child in &node.children {
        collect_leaf_tokens(child, tokens);
    }
}

fn multiset_intersection(left: &[String], right: &[String]) -> usize {
    let mut left_index = 0;
    let mut right_index = 0;
    let mut common = 0;
    while left_index < left.len() && right_index < right.len() {
        match left[left_index].cmp(&right[right_index]) {
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

fn nodes_equal(left: &SyntaxNode, right: &SyntaxNode) -> bool {
    left.kind == right.kind
        && left.text == right.text
        && left.children.len() == right.children.len()
        && left
            .children
            .iter()
            .zip(right.children.iter())
            .all(|(left, right)| nodes_equal(left, right))
}

fn signature(node: &SyntaxNode) -> String {
    if node.is_leaf() {
        // Keep leaf order aligned inside an already matched parent; leaf text
        // differences are reported as Modified by diff_node.
        return format!("{}#leaf", node.kind);
    }
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hash_node_content(node, &mut hasher);
    format!("{}#{:016x}", node.kind, hasher.finish())
}

fn hash_node_content(node: &SyntaxNode, hasher: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    node.kind.hash(hasher);
    if node.is_leaf() {
        node.text.hash(hasher);
    } else {
        for child in &node.children {
            hash_node_content(child, hasher);
        }
    }
}

fn push_subtree(
    result: &mut TreeDiffResult,
    max_chunks: usize,
    node: &SyntaxNode,
    path: &str,
    depth: usize,
    tag: TreeTag,
) {
    match tag {
        TreeTag::Added => result.added_nodes += node.node_count(),
        TreeTag::Removed => result.removed_nodes += node.node_count(),
        TreeTag::Modified => result.modified_nodes += 1,
        TreeTag::Moved => result.moved_nodes += node.node_count(),
    }
    push_chunk(
        result,
        max_chunks,
        TreeChunkResult {
            tag,
            path: path.to_string(),
            text: snippet(node, 400),
            depth,
        },
    );
}

fn push_chunk(result: &mut TreeDiffResult, max_chunks: usize, chunk: TreeChunkResult) {
    if result.chunks.len() < max_chunks {
        result.chunks.push(chunk);
    } else {
        result.truncated = true;
    }
}

fn snippet(node: &SyntaxNode, max_chars: usize) -> String {
    let mut parts = Vec::new();
    collect_leaf_text(node, &mut parts);
    truncate(&parts.join(" "), max_chars)
}

fn collect_leaf_text<'a>(node: &'a SyntaxNode, out: &mut Vec<&'a str>) {
    if node.is_leaf() {
        if !node.text.is_empty() {
            out.push(&node.text);
        }
        return;
    }
    for child in &node.children {
        collect_leaf_text(child, out);
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("...");
    out
}

#[cfg(test)]
#[path = "../../tests/unit/diff/tree_diff.rs"]
mod tests;
