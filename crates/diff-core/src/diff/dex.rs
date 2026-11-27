use droidsaw_dex::DexFile;
use droidsaw_dex::classes::decompile_class;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Decompile every class in a DEX file to Java source using droidsaw-dex.
///
/// Class output is sorted by class descriptor so DEX ordering changes do not
/// create noise. A panic in one class is isolated and recorded as a comment.
pub fn decompile_dex(bytes: &[u8]) -> Result<String, String> {
    let dex = DexFile::parse(bytes, None).map_err(|error| format!("DEX parse: {error:?}"))?;
    let mut classes = Vec::with_capacity(dex.class_defs.len());
    for class_def in &dex.class_defs {
        let descriptor = dex
            .get_type_descriptor(class_def.class_idx)
            .unwrap_or("L?;")
            .to_string();
        let source = catch_unwind(AssertUnwindSafe(|| decompile_class(&dex, bytes, class_def)))
            .unwrap_or_else(|_| format!("// decompilation panicked for {descriptor}\n"));
        classes.push((descriptor, source));
    }
    classes.sort_by(|left, right| left.0.cmp(&right.0));
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
    if output.is_empty() {
        return Err("DEX contains no classes".to_string());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(relative: &str) -> PathBuf {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop();
        path.pop();
        path.join("fixtures").join(relative)
    }

    #[test]
    fn decompiles_dex_to_java() {
        let bytes = std::fs::read(fixture("dex/test1.dex")).unwrap();
        let source = decompile_dex(&bytes).unwrap();
        assert!(source.contains("class "), "{source}");
        assert!(source.contains("BuildConfig"), "{source}");
    }
}
