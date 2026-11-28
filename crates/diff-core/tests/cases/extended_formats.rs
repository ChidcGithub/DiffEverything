use diff_core::model::{DiffBody, DiffStatus};
use diff_core::{DiffEngine, DiffOptions};
use serde_json::Value;
use std::path::PathBuf;

fn engine() -> DiffEngine {
    DiffEngine::new(DiffOptions::default())
}

fn fixture(relative: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.join("fixtures").join(relative)
}

fn structured(report: &diff_core::model::DiffReport) -> &diff_core::model::StructuredDiff {
    match &report.root.body {
        DiffBody::Structured(diff) => diff,
        other => panic!("expected structured diff, got {other:?}"),
    }
}

fn structured_count(report: &diff_core::model::DiffReport) -> usize {
    structured(report).changes.len()
}

fn document_count(report: &diff_core::model::DiffReport) -> usize {
    match &report.root.body {
        DiffBody::Document(diff) => {
            diff.blocks.len() + diff.tables.len() + diff.metadata.len() + diff.media.len()
        }
        other => panic!("expected document diff, got {other:?}"),
    }
}

fn assert_document_detail_modes(relative_left: &str, relative_right: &str, format: &str) {
    let report = engine()
        .compare(&fixture(relative_left), &fixture(relative_right))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Document(diff) => assert_eq!(diff.format, format),
        other => panic!("expected {format} document diff, got {other:?}"),
    }
    let full = document_count(&report);
    assert!(
        full >= 2,
        "{format}: expected at least two changes, got {full}"
    );
    let mut concise = report.clone();
    diff_core::concise::apply(&mut concise, 10);
    let brief = document_count(&concise);
    assert!(
        brief <= full,
        "{format}: concise mode increased changes ({brief} vs {full})"
    );
}

fn assert_detail_modes(relative_left: &str, relative_right: &str, format: &str) {
    let report = engine()
        .compare(&fixture(relative_left), &fixture(relative_right))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert_eq!(structured(&report).format, format);
    let full = structured_count(&report);
    assert!(
        full >= 5,
        "{format}: expected at least five changes, got {full}"
    );

    let mut concise = report.clone();
    diff_core::concise::apply(&mut concise, 4);
    let brief = structured_count(&concise);
    assert!(
        brief < full,
        "{format}: concise mode must show fewer changes ({brief} vs {full})"
    );
    assert!(brief <= 4, "{format}: concise mode kept {brief} changes");
}

#[test]
fn compares_parquet_semantically_and_respects_detail() {
    let report = engine()
        .compare(
            &fixture("tabular/old.parquet"),
            &fixture("tabular/new.parquet"),
        )
        .unwrap();
    assert!(structured(&report).changes.iter().any(|change| {
        change.path == "$.rows[1].name"
            && change.left == Some(Value::String("beta".into()))
            && change.right == Some(Value::String("beta-updated".into()))
    }));
    assert_detail_modes("tabular/old.parquet", "tabular/new.parquet", "parquet");
}

#[test]
fn compares_avro_semantically_and_respects_detail() {
    let report = engine()
        .compare(&fixture("tabular/old.avro"), &fixture("tabular/new.avro"))
        .unwrap();
    assert!(structured(&report).changes.iter().any(|change| {
        change.path == "$.rows[1].score"
            && change.left == Some(Value::from(20.0))
            && change.right == Some(Value::from(25.0))
    }));
    assert_detail_modes("tabular/old.avro", "tabular/new.avro", "avro");
}

#[test]
fn compares_orc_semantically_and_respects_detail() {
    let report = engine()
        .compare(&fixture("tabular/old.orc"), &fixture("tabular/new.orc"))
        .unwrap();
    assert!(structured(&report).changes.iter().any(|change| {
        change.path == "$.stripe_region_sha256"
            && change.left.is_some()
            && change.right.is_some()
            && change.left != change.right
    }));
    assert_detail_modes("tabular/old.orc", "tabular/new.orc", "orc");
}

#[test]
fn compares_cbor_semantically_and_respects_detail() {
    let report = engine()
        .compare(
            &fixture("serialization/old.cbor"),
            &fixture("serialization/new.cbor"),
        )
        .unwrap();
    assert!(structured(&report).changes.iter().any(|change| {
        change.path == "$.meta.active"
            && change.left == Some(Value::Bool(true))
            && change.right == Some(Value::Bool(false))
    }));
    assert_detail_modes("serialization/old.cbor", "serialization/new.cbor", "cbor");
}

#[test]
fn compares_messagepack_semantically_and_respects_detail() {
    let report = engine()
        .compare(
            &fixture("serialization/old.msgpack"),
            &fixture("serialization/new.msgpack"),
        )
        .unwrap();
    assert!(structured(&report).changes.iter().any(|change| {
        change.path == "$.tags[1]"
            && change.left == Some(Value::String("b".into()))
            && change.right == Some(Value::String("c".into()))
    }));
    assert_detail_modes(
        "serialization/old.msgpack",
        "serialization/new.msgpack",
        "messagepack",
    );
}

#[test]
fn compares_protobuf_semantically_and_respects_detail() {
    let report = engine()
        .compare(
            &fixture("serialization/old.pb"),
            &fixture("serialization/new.pb"),
        )
        .unwrap();
    assert!(structured(&report).changes.iter().any(|change| {
        change.path == "$.field_2"
            && change.left == Some(Value::String("alpha".into()))
            && change.right == Some(Value::String("beta".into()))
    }));
    assert_detail_modes("serialization/old.pb", "serialization/new.pb", "protobuf");
}

#[test]
fn compares_odt_semantically_and_respects_detail() {
    let report = engine()
        .compare(&fixture("documents/old.odt"), &fixture("documents/new.odt"))
        .unwrap();
    match &report.root.body {
        DiffBody::Document(diff) => {
            assert_eq!(diff.format, "odt");
            assert!(diff.metadata.iter().any(|change| {
                change.path == "metadata/title"
                    && change.left.as_deref() == Some("Old Doc")
                    && change.right.as_deref() == Some("New Doc")
            }));
            assert!(
                diff.blocks
                    .iter()
                    .any(|block| block.path.starts_with("body/p["))
            );
        }
        other => panic!("expected ODT document diff, got {other:?}"),
    }
    assert_document_detail_modes("documents/old.odt", "documents/new.odt", "odt");
}

#[test]
fn compares_odp_semantically_and_respects_detail() {
    let report = engine()
        .compare(&fixture("documents/old.odp"), &fixture("documents/new.odp"))
        .unwrap();
    match &report.root.body {
        DiffBody::Document(diff) => {
            assert_eq!(diff.format, "odp");
            assert!(
                diff.blocks
                    .iter()
                    .any(|block| block.path.starts_with("slide["))
            );
        }
        other => panic!("expected ODP document diff, got {other:?}"),
    }
    assert_document_detail_modes("documents/old.odp", "documents/new.odp", "odp");
}

#[test]
fn compares_epub_semantically_and_respects_detail() {
    let report = engine()
        .compare(
            &fixture("documents/old.epub"),
            &fixture("documents/new.epub"),
        )
        .unwrap();
    assert!(structured(&report).changes.iter().any(|change| {
        change.path == "$.chapters[0].text"
            && change.left.is_some()
            && change.right.is_some()
            && change.left != change.right
    }));
    assert_detail_modes("documents/old.epub", "documents/new.epub", "epub");
}

#[test]
fn compares_rtf_semantically_and_respects_detail() {
    let report = engine()
        .compare(&fixture("documents/old.rtf"), &fixture("documents/new.rtf"))
        .unwrap();
    assert!(structured(&report).changes.iter().any(|change| {
        change.path == "$.paragraphs[0]"
            && change.left.is_some()
            && change.right.is_some()
            && change.left != change.right
    }));
    assert_detail_modes("documents/old.rtf", "documents/new.rtf", "rtf");
}

#[test]
fn compares_eml_semantically_and_respects_detail() {
    let report = engine()
        .compare(&fixture("documents/old.eml"), &fixture("documents/new.eml"))
        .unwrap();
    assert!(structured(&report).changes.iter().any(|change| {
        change.path == "$.headers.subject"
            && change.left == Some(Value::String("Old subject".into()))
            && change.right == Some(Value::String("New subject".into()))
    }));
    assert!(structured(&report).changes.iter().any(|change| {
        change.path == "$.attachments[0].sha256"
            && change.left.is_some()
            && change.right.is_some()
            && change.left != change.right
    }));
    assert_detail_modes("documents/old.eml", "documents/new.eml", "eml");
}

#[test]
fn compares_squashfs_semantically_and_respects_detail() {
    let report = engine()
        .compare(
            &fixture("packages/old.squashfs"),
            &fixture("packages/new.squashfs"),
        )
        .unwrap();
    assert!(structured(&report).changes.iter().any(|change| {
        change.path == "$.entries[3].text"
            && change.left.is_some()
            && change.right.is_some()
            && change.left != change.right
    }));
    assert_detail_modes("packages/old.squashfs", "packages/new.squashfs", "squashfs");
}

#[test]
fn compares_snap_semantically_and_respects_detail() {
    let report = engine()
        .compare(&fixture("packages/old.snap"), &fixture("packages/new.snap"))
        .unwrap();
    assert!(structured(&report).changes.iter().any(|change| {
        change.path == "$.entries[1].target"
            && change.left == Some(Value::String("broken".into()))
            && change.right == Some(Value::String("really-broken".into()))
    }));
    assert_detail_modes("packages/old.snap", "packages/new.snap", "snap");
}

#[test]
fn compares_appimage_semantically_and_respects_detail() {
    let report = engine()
        .compare(
            &fixture("packages/old.appimage"),
            &fixture("packages/new.appimage"),
        )
        .unwrap();
    assert!(structured(&report).changes.iter().any(|change| {
        change.path == "$.entries[3].sha256"
            && change.left.is_some()
            && change.right.is_some()
            && change.left != change.right
    }));
    assert_detail_modes("packages/old.appimage", "packages/new.appimage", "appimage");
}

#[test]
fn compares_ext4_semantically_and_respects_detail() {
    let report = engine()
        .compare(&fixture("packages/old.ext4"), &fixture("packages/new.ext4"))
        .unwrap();
    assert!(structured(&report).changes.iter().any(|change| {
        change.path == "$.entries[1].text"
            && change.left.is_some()
            && change.right.is_some()
            && change.left != change.right
    }));
    assert_detail_modes("packages/old.ext4", "packages/new.ext4", "ext4");
}

#[test]
fn compares_dmg_semantically_and_respects_detail() {
    let report = engine()
        .compare(&fixture("packages/old.dmg"), &fixture("packages/new.dmg"))
        .unwrap();
    assert!(structured(&report).changes.iter().any(|change| {
        change.path == "$.block_maps[0].checksum"
            && change.left.is_some()
            && change.right.is_some()
            && change.left != change.right
    }));
    assert_detail_modes("packages/old.dmg", "packages/new.dmg", "dmg");
}

#[test]
fn compares_msix_semantically_and_respects_detail() {
    let report = engine()
        .compare(&fixture("packages/old.msix"), &fixture("packages/new.msix"))
        .unwrap();
    assert!(structured(&report).changes.iter().any(|change| {
        change.path == "$.manifest.children[0].attributes.Version"
            && change.left == Some(Value::String("1.0.0.0".into()))
            && change.right == Some(Value::String("1.0.0.1".into()))
    }));
    assert_detail_modes("packages/old.msix", "packages/new.msix", "msix");
}

#[test]
fn compares_aab_semantically_and_respects_detail() {
    let report = engine()
        .compare(&fixture("packages/old.aab"), &fixture("packages/new.aab"))
        .unwrap();
    assert!(structured(&report).changes.iter().any(|change| {
        change.path == "$.bundle_config.field_4"
            && change.left == Some(Value::from(1))
            && change.right == Some(Value::from(2))
    }));
    assert_detail_modes("packages/old.aab", "packages/new.aab", "aab");
}

#[test]
fn compares_flatpak_semantically_and_respects_detail() {
    let report = engine()
        .compare(
            &fixture("packages/old.flatpak"),
            &fixture("packages/new.flatpak"),
        )
        .unwrap();
    assert!(structured(&report).changes.iter().any(|change| {
        change.path == "$.refs[0]"
            && change.left.is_some()
            && change.right.is_some()
            && change.left != change.right
    }));
    assert_detail_modes("packages/old.flatpak", "packages/new.flatpak", "flatpak");
}

#[test]
fn compares_video_semantically_and_respects_detail() {
    let report = engine()
        .compare(&fixture("media/old.mp4"), &fixture("media/new.mp4"))
        .unwrap();
    assert!(structured(&report).changes.iter().any(|change| {
        change.path == "$.track_stats[0].packet_sha256"
            && change.left.is_some()
            && change.right.is_some()
            && change.left != change.right
    }));
    assert_detail_modes("media/old.mp4", "media/new.mp4", "video");
}

#[test]
fn compares_audio_waveform_and_respects_detail() {
    let report = engine()
        .compare(&fixture("media/old.wav"), &fixture("media/new.wav"))
        .unwrap();
    let waveform = match &report.root.body {
        DiffBody::Audio(diff) => diff.waveform.as_ref().expect("waveform"),
        other => panic!("expected audio diff, got {other:?}"),
    };
    let full_windows = waveform.left.len();
    assert!(full_windows >= 5, "expected at least five waveform windows");
    assert!(waveform.changed_windows >= 5);

    let mut concise = report.clone();
    diff_core::concise::apply(&mut concise, 4);
    let concise_windows = match &concise.root.body {
        DiffBody::Audio(diff) => diff.waveform.as_ref().expect("waveform").left.len(),
        other => panic!("expected audio diff, got {other:?}"),
    };
    assert!(
        concise_windows < full_windows,
        "concise waveform must show fewer windows ({concise_windows} vs {full_windows})"
    );
}
