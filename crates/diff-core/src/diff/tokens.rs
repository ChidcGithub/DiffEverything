use crate::model::TextTag;
use similar::{Algorithm, DiffOp, capture_diff_slices};

#[derive(Debug, Clone)]
pub struct TokenDiffResult {
    pub language: String,
    pub added_tokens: usize,
    pub removed_tokens: usize,
    pub chunks: Vec<TokenChunkResult>,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub struct TokenChunkResult {
    pub tag: TextTag,
    pub text: String,
}

#[derive(Debug, Clone, Copy)]
pub struct TokenizeOptions {
    pub ignore_comments: bool,
    pub max_tokens: usize,
}

impl Default for TokenizeOptions {
    fn default() -> Self {
        Self {
            ignore_comments: false,
            max_tokens: 200_000,
        }
    }
}

pub struct Token {
    pub text: String,
    pub kind: String,
}

pub fn language_for_name(name: &str) -> Option<(&'static str, tree_sitter::Language)> {
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, ext)| ext).unwrap_or(&lower);
    let language = match ext {
        "rs" => ("rust", tree_sitter_rust::LANGUAGE.into()),
        "py" | "pyi" => ("python", tree_sitter_python::LANGUAGE.into()),
        "js" | "jsx" | "mjs" | "cjs" => ("javascript", tree_sitter_javascript::LANGUAGE.into()),
        "ts" => (
            "typescript",
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        ),
        "tsx" => ("tsx", tree_sitter_typescript::LANGUAGE_TSX.into()),
        "c" | "h" => ("c", tree_sitter_c::LANGUAGE.into()),
        "cc" | "cpp" | "cxx" | "hpp" | "hh" => ("cpp", tree_sitter_cpp::LANGUAGE.into()),
        "go" => ("go", tree_sitter_go::LANGUAGE.into()),
        "java" => ("java", tree_sitter_java::LANGUAGE.into()),
        "sh" | "bash" | "zsh" | "fish" => ("bash", tree_sitter_bash::LANGUAGE.into()),
        "rb" => ("ruby", tree_sitter_ruby::LANGUAGE.into()),
        "cs" => ("csharp", tree_sitter_c_sharp::LANGUAGE.into()),
        "swift" => ("swift", tree_sitter_swift::LANGUAGE.into()),
        "lua" => ("lua", tree_sitter_lua::LANGUAGE.into()),
        _ => return None,
    };
    Some(language)
}

pub fn tokenize(
    source: &str,
    language: &tree_sitter::Language,
    options: TokenizeOptions,
) -> std::result::Result<Vec<Token>, String> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(language)
        .map_err(|error| error.to_string())?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "tree-sitter returned no tree".to_string())?;

    let mut tokens = Vec::new();
    collect_tokens(tree.root_node(), source.as_bytes(), options, &mut tokens);
    if tokens.len() > options.max_tokens {
        return Err(format!(
            "token limit exceeded ({} > {})",
            tokens.len(),
            options.max_tokens
        ));
    }
    Ok(tokens)
}

fn collect_tokens(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    options: TokenizeOptions,
    out: &mut Vec<Token>,
) {
    if options.ignore_comments && node.kind().contains("comment") {
        return;
    }
    if node.child_count() == 0 {
        let kind = node.kind();
        let text = node.utf8_text(source).unwrap_or("").trim();
        if !text.is_empty() {
            out.push(Token {
                text: text.to_string(),
                kind: kind.to_string(),
            });
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_tokens(child, source, options, out);
    }
}

pub fn compare_token_streams(
    language: &str,
    left: &[Token],
    right: &[Token],
    max_chunks: usize,
) -> TokenDiffResult {
    let left_texts: Vec<&str> = left.iter().map(|token| token.text.as_str()).collect();
    let right_texts: Vec<&str> = right.iter().map(|token| token.text.as_str()).collect();
    let ops = capture_diff_slices(Algorithm::Myers, &left_texts, &right_texts);

    let mut chunks = Vec::new();
    let mut added_tokens = 0;
    let mut removed_tokens = 0;
    let mut truncated = false;

    for op in ops {
        match op {
            DiffOp::Equal {
                old_index,
                len,
                new_index: _,
            } => {
                if len == 0 {
                    continue;
                }
                push_chunk(
                    &mut chunks,
                    &mut truncated,
                    max_chunks,
                    TextTag::Equal,
                    join_tokens(&left_texts[old_index..old_index + len]),
                );
            }
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                removed_tokens += old_len;
                push_chunk(
                    &mut chunks,
                    &mut truncated,
                    max_chunks,
                    TextTag::Delete,
                    join_tokens(&left_texts[old_index..old_index + old_len]),
                );
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                added_tokens += new_len;
                push_chunk(
                    &mut chunks,
                    &mut truncated,
                    max_chunks,
                    TextTag::Insert,
                    join_tokens(&right_texts[new_index..new_index + new_len]),
                );
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                removed_tokens += old_len;
                added_tokens += new_len;
                push_chunk(
                    &mut chunks,
                    &mut truncated,
                    max_chunks,
                    TextTag::Delete,
                    join_tokens(&left_texts[old_index..old_index + old_len]),
                );
                push_chunk(
                    &mut chunks,
                    &mut truncated,
                    max_chunks,
                    TextTag::Insert,
                    join_tokens(&right_texts[new_index..new_index + new_len]),
                );
            }
        }
    }

    TokenDiffResult {
        language: language.to_string(),
        added_tokens,
        removed_tokens,
        chunks,
        truncated,
    }
}

fn push_chunk(
    chunks: &mut Vec<TokenChunkResult>,
    truncated: &mut bool,
    max_chunks: usize,
    tag: TextTag,
    text: String,
) {
    if text.is_empty() {
        return;
    }
    if chunks.len() >= max_chunks {
        *truncated = true;
        return;
    }
    chunks.push(TokenChunkResult { tag, text });
}

fn join_tokens(tokens: &[&str]) -> String {
    if tokens.len() <= 60 {
        return tokens.join(" ");
    }
    let head = tokens[..30].join(" ");
    let tail = tokens[tokens.len() - 30..].join(" ");
    format!("{head} ... {tail}")
}

#[cfg(test)]
#[path = "../../tests/unit/diff/tokens.rs"]
mod tests;
