use crate::diff::text;
use crate::model::TextDiff;
use droidsaw_dex::DexFile;
use droidsaw_dex::classes::decompile_class;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Decompile every class in a DEX file to Java source using droidsaw-dex.
///
/// Classes are decompiled in parallel because large multidex APKs contain
/// thousands of independent classes. A panic in one class is isolated and
/// recorded as a comment.
pub fn decompile_dex_classes(bytes: &[u8]) -> Result<BTreeMap<String, String>, String> {
    let dex = DexFile::parse(bytes, None).map_err(|error| format!("DEX parse: {error:?}"))?;
    let classes: Vec<(String, String)> = dex
        .class_defs
        .par_iter()
        .map(|class_def| {
            let descriptor = dex
                .get_type_descriptor(class_def.class_idx)
                .unwrap_or("L?;")
                .to_string();
            let source = catch_unwind(AssertUnwindSafe(|| decompile_class(&dex, bytes, class_def)))
                .unwrap_or_else(|_| format!("// decompilation panicked for {descriptor}\n"));
            (descriptor, source)
        })
        .collect();
    let mut classes_by_descriptor = BTreeMap::new();
    for (descriptor, source) in classes {
        classes_by_descriptor.insert(descriptor, source);
    }
    if classes_by_descriptor.is_empty() {
        return Err("DEX contains no classes".to_string());
    }
    Ok(classes_by_descriptor)
}

/// Decompile a DEX file into one Java source listing sorted by descriptor.
///
/// This keeps the historical single-string API for tests and callers that
/// need the full source. Comparison uses [`compare_dex`] instead so unchanged
/// classes can be collapsed before the line diff.
pub fn decompile_dex(bytes: &[u8]) -> Result<String, String> {
    let classes = decompile_dex_classes(bytes)?;
    let mut output = String::new();
    for (descriptor, source) in classes {
        output.push_str("// class ");
        output.push_str(&descriptor);
        output.push('\n');
        output.push_str(&source);
        if !source.ends_with('\n') {
            output.push('\n');
        }
        output.push('\n');
    }
    Ok(output)
}

/// Compare two DEX files at class granularity.
///
/// Decompiling every class and then running a line diff over both full
/// listings is extremely expensive for multidex APKs: the decompiled text can
/// exceed 60 MB and contain millions of lines. Classes with identical
/// decompiled source are therefore omitted before the line diff, and only
/// changed, added or removed classes are rendered. The resulting report still
/// contains every actual code change, with a summary of how many classes were
/// unchanged.
const FULL_SOURCE_CLASS_LIMIT: usize = 64;
const MAX_LISTED_CLASSES: usize = 200;
const MAX_CHANGED_CLASSES: usize = 100;

/// Compare two DEX files at class granularity.
///
/// Decompiling every class and running a line diff over both full listings is
/// extremely expensive for multidex APKs: the decompiled text can exceed 60 MB
/// and millions of lines. This comparison first parses both DEX files, then:
///
/// - decompiles only classes present on both sides, so changed classes can be
///   compared as Java source;
/// - lists descriptors for large one-sided class sets instead of decompiling
///   thousands of added/removed classes;
/// - caps the number of changed classes included in the text report.
///
/// Every actual class-level change is still summarized. For small APKs, full
/// source is kept for one-sided classes as before.
pub fn compare_dex(
    left_bytes: &[u8],
    right_bytes: &[u8],
    context: usize,
    max_hunks: usize,
) -> Result<TextDiff, String> {
    let left_dex =
        DexFile::parse(left_bytes, None).map_err(|error| format!("DEX parse (left): {error:?}"))?;
    let right_dex = DexFile::parse(right_bytes, None)
        .map_err(|error| format!("DEX parse (right): {error:?}"))?;
    let left_classes = class_indices(&left_dex);
    let right_classes = class_indices(&right_dex);

    let common: Vec<(String, usize, usize)> = left_classes
        .iter()
        .filter_map(|(descriptor, left_index)| {
            right_classes
                .get(descriptor)
                .map(|right_index| (descriptor.clone(), *left_index, *right_index))
        })
        .collect();
    let only_left: Vec<String> = left_classes
        .keys()
        .filter(|descriptor| !right_classes.contains_key(*descriptor))
        .cloned()
        .collect();
    let only_right: Vec<String> = right_classes
        .keys()
        .filter(|descriptor| !left_classes.contains_key(*descriptor))
        .cloned()
        .collect();

    let left_full_source = only_left.len() <= FULL_SOURCE_CLASS_LIMIT;
    let right_full_source = only_right.len() <= FULL_SOURCE_CLASS_LIMIT;

    let mut left_jobs: Vec<usize> = common.iter().map(|(_, left, _)| *left).collect();
    let mut right_jobs: Vec<usize> = common.iter().map(|(_, _, right)| *right).collect();
    if left_full_source {
        left_jobs.extend(
            only_left
                .iter()
                .filter_map(|descriptor| left_classes.get(descriptor).copied()),
        );
    }
    if right_full_source {
        right_jobs.extend(
            only_right
                .iter()
                .filter_map(|descriptor| right_classes.get(descriptor).copied()),
        );
    }

    let left_sources = decompile_indices(&left_dex, left_bytes, &left_jobs);
    let right_sources = decompile_indices(&right_dex, right_bytes, &right_jobs);
    let changed: Vec<(String, usize, usize)> = common
        .iter()
        .filter(|(descriptor, _, _)| left_sources.get(descriptor) != right_sources.get(descriptor))
        .cloned()
        .collect();

    let total = left_classes.len() + only_right.len();
    let mut summary = format!(
        "// DEX class summary: total={} common={} changed={} only-left={} only-right={}\n\
         // unchanged classes are omitted from this report\n",
        total,
        common.len(),
        changed.len(),
        only_left.len(),
        only_right.len()
    );
    if !left_full_source || !right_full_source {
        summary.push_str(
            "// large one-sided class sets are listed by descriptor; class bodies are omitted\n",
        );
    }
    let mut left_view = summary.clone();
    let mut right_view = summary;

    if !only_left.is_empty() {
        let header = format!("// classes only in left: {}\n", only_left.len());
        left_view.push_str(&header);
        right_view.push_str(&header);
        if left_full_source {
            for descriptor in &only_left {
                if let Some(source) = left_sources.get(descriptor) {
                    push_class(&mut left_view, &format!("// class {descriptor}\n"), source);
                }
            }
        } else {
            for descriptor in only_left.iter().take(MAX_LISTED_CLASSES) {
                left_view.push_str("// ");
                left_view.push_str(descriptor);
                left_view.push('\n');
            }
            if only_left.len() > MAX_LISTED_CLASSES {
                let note = format!(
                    "// ... {} more only-left classes\n",
                    only_left.len() - MAX_LISTED_CLASSES
                );
                left_view.push_str(&note);
                right_view.push_str(&note);
            }
        }
    }

    if !only_right.is_empty() {
        let header = format!("// classes only in right: {}\n", only_right.len());
        left_view.push_str(&header);
        right_view.push_str(&header);
        if right_full_source {
            for descriptor in &only_right {
                if let Some(source) = right_sources.get(descriptor) {
                    push_class(&mut right_view, &format!("// class {descriptor}\n"), source);
                }
            }
        } else {
            for descriptor in only_right.iter().take(MAX_LISTED_CLASSES) {
                right_view.push_str("// ");
                right_view.push_str(descriptor);
                right_view.push('\n');
            }
            if only_right.len() > MAX_LISTED_CLASSES {
                let note = format!(
                    "// ... {} more only-right classes\n",
                    only_right.len() - MAX_LISTED_CLASSES
                );
                left_view.push_str(&note);
                right_view.push_str(&note);
            }
        }
    }

    if !changed.is_empty() {
        let header = format!("// changed classes: {}\n", changed.len());
        left_view.push_str(&header);
        right_view.push_str(&header);
        for (descriptor, _, _) in changed.iter().take(MAX_CHANGED_CLASSES) {
            if let Some(source) = left_sources.get(descriptor) {
                push_class(&mut left_view, &format!("// class {descriptor}\n"), source);
            }
            if let Some(source) = right_sources.get(descriptor) {
                push_class(&mut right_view, &format!("// class {descriptor}\n"), source);
            }
        }
        if changed.len() > MAX_CHANGED_CLASSES {
            let note = format!(
                "// ... {} more changed classes omitted\n",
                changed.len() - MAX_CHANGED_CLASSES
            );
            left_view.push_str(&note);
            right_view.push_str(&note);
        }
    }

    let mut diff =
        text::compare_text_options(&left_view, &right_view, context, max_hunks, false, false);
    diff.format = "dex-java".to_string();
    diff.encoding_left = "utf-8".to_string();
    diff.encoding_right = "utf-8".to_string();
    Ok(diff)
}

fn class_indices(dex: &DexFile) -> BTreeMap<String, usize> {
    dex.class_defs
        .iter()
        .enumerate()
        .filter_map(|(index, class_def)| {
            dex.get_type_descriptor(class_def.class_idx)
                .ok()
                .map(|descriptor| (descriptor.to_string(), index))
        })
        .collect()
}

fn decompile_indices(dex: &DexFile, bytes: &[u8], indices: &[usize]) -> BTreeMap<String, String> {
    indices
        .par_iter()
        .filter_map(|index| {
            let class_def = dex.class_defs.get(*index)?;
            let descriptor = dex
                .get_type_descriptor(class_def.class_idx)
                .unwrap_or("L?;")
                .to_string();
            let source = catch_unwind(AssertUnwindSafe(|| decompile_class(dex, bytes, class_def)))
                .unwrap_or_else(|_| format!("// decompilation panicked for {descriptor}\n"));
            Some((descriptor, source))
        })
        .collect()
}

fn push_class(output: &mut String, header: &str, source: &str) {
    output.push_str(header);
    output.push_str(source);
    if !source.is_empty() && !source.ends_with('\n') {
        output.push('\n');
    }
    output.push('\n');
}

#[cfg(test)]
#[path = "../../tests/unit/diff/dex.rs"]
mod tests;
