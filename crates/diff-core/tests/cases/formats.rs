use diff_core::diff::notebook::{notebook_cell, notebook_json};
use diff_core::model::{DiffBody, DiffStatus, RowChangeKind, StructuredChangeKind};
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

fn external(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.join("testdata").join("external").join(name)
}

#[test]
fn compares_woff2_fonts_semantically() {
    let report = engine()
        .compare(&fixture("font/regular.woff2"), &fixture("font/bold.woff2"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "font");
            assert!(diff.changes.iter().any(|change| {
                change.path == "$.weight"
                    && change.left == Some(Value::from(400))
                    && change.right == Some(Value::from(700))
            }));
        }
        other => panic!("expected structured woff2 diff, got {other:?}"),
    }

    let cross_format = engine()
        .compare(&fixture("font/regular.ttf"), &fixture("font/regular.woff2"))
        .unwrap();
    assert_eq!(cross_format.status, DiffStatus::Equal);
}

#[test]
fn compares_woff_fonts_semantically() {
    let report = engine()
        .compare(&fixture("font/regular.woff"), &fixture("font/bold.woff"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "font");
            assert!(diff.changes.iter().any(|change| {
                change.path == "$.weight"
                    && change.left == Some(Value::from(400))
                    && change.right == Some(Value::from(700))
            }));
        }
        other => panic!("expected structured woff diff, got {other:?}"),
    }

    let cross_format = engine()
        .compare(&fixture("font/regular.ttf"), &fixture("font/regular.woff"))
        .unwrap();
    assert_eq!(cross_format.status, DiffStatus::Equal);
}

#[test]
fn compares_xar_archives() {
    let report = engine()
        .compare(&fixture("xar/left.xar"), &fixture("xar/right.xar"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Archive(diff) => {
            assert_eq!(diff.format, "xar");
            assert_eq!(diff.total_entries, 4);
            assert!(diff.entries.iter().any(|entry| {
                entry.label == "hello.txt" && entry.status == DiffStatus::Different
            }));
            assert!(diff.entries.iter().any(|entry| {
                entry.label == "logs/app.log" && entry.status == DiffStatus::Different
            }));
            assert!(diff.entries.iter().any(|entry| {
                entry.label == "docs/data.bin" && entry.status == DiffStatus::Different
            }));
            let readme = diff
                .entries
                .iter()
                .find(|entry| entry.label == "docs/readme.md")
                .expect("readme entry");
            assert_eq!(readme.status, DiffStatus::Equal);
        }
        other => panic!("expected xar archive diff, got {other:?}"),
    }

    let identical = engine()
        .compare(&fixture("xar/left.xar"), &fixture("xar/left.xar"))
        .unwrap();
    assert_eq!(identical.status, DiffStatus::Equal);
}

#[test]
fn compares_xar_compression_codecs() {
    let report = engine()
        .compare(
            &fixture("xar/mixed-left.xar"),
            &fixture("xar/mixed-right.xar"),
        )
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Archive(diff) => {
            assert_eq!(diff.format, "xar");
            assert_eq!(diff.total_entries, 5);
            for codec in ["zlib", "bzip2", "xz", "lzma", "none"] {
                let label = format!("codec/{codec}.txt");
                let entry = diff
                    .entries
                    .iter()
                    .find(|entry| entry.label == label)
                    .unwrap_or_else(|| panic!("missing {label}"));
                assert_eq!(entry.status, DiffStatus::Different);
            }
        }
        other => panic!("expected xar archive diff, got {other:?}"),
    }
}

#[test]
fn compares_xar_symlinks() {
    let report = engine()
        .compare(
            &fixture("xar/links-left.xar"),
            &fixture("xar/links-right.xar"),
        )
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Archive(diff) => {
            assert_eq!(diff.format, "xar");
            assert_eq!(diff.total_entries, 2);
            let changed = diff
                .entries
                .iter()
                .find(|entry| entry.label == "target.txt")
                .expect("target link");
            assert_eq!(changed.status, DiffStatus::Different);
            let same = diff
                .entries
                .iter()
                .find(|entry| entry.label == "same.txt")
                .expect("same link");
            assert_eq!(same.status, DiffStatus::Equal);
        }
        other => panic!("expected xar archive diff, got {other:?}"),
    }
}

#[test]
fn compares_ar_archives() {
    let report = engine()
        .compare(&fixture("ar/test1.a"), &fixture("ar/test2.a"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Archive(diff) => {
            assert_eq!(diff.format, "ar");
            assert_eq!(diff.total_entries, 1);
            let entry = &diff.entries[0];
            assert!(entry.label.ends_with("test.o"));
            assert_eq!(entry.status, DiffStatus::Different);
        }
        other => panic!("expected ar archive diff, got {other:?}"),
    }

    let identical = engine()
        .compare(&fixture("ar/test1.a"), &fixture("ar/test1.a"))
        .unwrap();
    assert_eq!(identical.status, DiffStatus::Equal);
}

#[test]
fn compares_bsd_ar_archives() {
    let report = engine()
        .compare(&fixture("ar/bsd-old.a"), &fixture("ar/bsd-new.a"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Archive(diff) => {
            assert_eq!(diff.format, "ar");
            assert_eq!(diff.total_entries, 2);
            assert!(diff.entries.iter().any(|entry| {
                entry.label == "long_member_name_one.txt" && entry.status == DiffStatus::Different
            }));
            assert!(diff.entries.iter().any(|entry| {
                entry.label == "second_member.txt" && entry.status == DiffStatus::Equal
            }));
        }
        other => panic!("expected bsd ar archive diff, got {other:?}"),
    }
}

#[test]
fn compares_cab_archives() {
    let report = engine()
        .compare(&fixture("cab/old.cab"), &fixture("cab/new.cab"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Archive(diff) => {
            assert_eq!(diff.format, "cab");
            assert_eq!(diff.total_entries, 3);
            assert!(diff.entries.iter().any(|entry| {
                entry.label.ends_with("data.txt") && entry.status == DiffStatus::Different
            }));
            assert!(diff.entries.iter().any(|entry| {
                entry.label.ends_with("extra.txt") && entry.status == DiffStatus::Different
            }));
        }
        other => panic!("expected cab archive diff, got {other:?}"),
    }
}

#[test]
fn compares_fonts_semantically() {
    let report = engine()
        .compare(&fixture("font/regular.ttf"), &fixture("font/bold.ttf"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "font");
            assert!(diff.changes.iter().any(|change| {
                change.path == "$.weight"
                    && change.left == Some(Value::from(400))
                    && change.right == Some(Value::from(700))
            }));
            assert!(diff.changes.iter().any(|change| {
                change.path == "$.bold"
                    && change.left == Some(Value::Bool(false))
                    && change.right == Some(Value::Bool(true))
            }));
            assert!(
                diff.changes
                    .iter()
                    .any(|change| change.path.starts_with("$.ascii_advances")),
                "{:?}",
                diff.changes
            );
        }
        other => panic!("expected structured font diff, got {other:?}"),
    }

    let equal = engine()
        .compare(&fixture("font/regular.ttf"), &fixture("font/regular.ttf"))
        .unwrap();
    assert_eq!(equal.status, DiffStatus::Equal);
}

#[test]
fn compares_elf_executables_semantically() {
    let report = engine()
        .compare(
            &fixture("executable/hello-old"),
            &fixture("executable/hello-new"),
        )
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "elf");
            assert!(
                diff.changes
                    .iter()
                    .any(|change| change.path.starts_with("$.sections")),
                "{:?}",
                diff.changes
            );
            assert!(
                diff.changes
                    .iter()
                    .any(|change| change.path.starts_with("$.imports")),
                "{:?}",
                diff.changes
            );
            assert!(
                diff.changes.iter().any(|change| {
                    change.path.contains(".byte_diff")
                        && change.left.as_ref().is_some_and(|value| {
                            value["absolute_offset"].is_u64() && value["hex"].is_string()
                        })
                }),
                "{:?}",
                diff.changes
            );
        }
        other => panic!("expected structured elf diff, got {other:?}"),
    }

    let equal = engine()
        .compare(
            &fixture("executable/hello-old"),
            &fixture("executable/hello-old"),
        )
        .unwrap();
    assert_eq!(equal.status, DiffStatus::Equal);
}

#[test]
fn compares_macho_executables_semantically() {
    let report = engine()
        .compare(
            &fixture("executable/diffoscope-test1.macho"),
            &fixture("executable/diffoscope-test1-mod.macho"),
        )
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "mach-o-fat");
            assert!(
                diff.changes
                    .iter()
                    .any(|change| change.path.starts_with("$.architectures[0].sections")),
                "{:?}",
                diff.changes
            );
        }
        other => panic!("expected structured mach-o diff, got {other:?}"),
    }
}

#[test]
fn compares_pe_fixtures_semantically() {
    let report = engine()
        .compare(
            &fixture("executable/diffoscope-test1.exe"),
            &fixture("executable/diffoscope-test2.exe"),
        )
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "pe");
            assert!(
                diff.changes
                    .iter()
                    .any(|change| change.path.starts_with("$.sections")),
                "{:?}",
                diff.changes
            );
        }
        other => panic!("expected structured pe fixture diff, got {other:?}"),
    }
}

#[test]
fn compares_pe_executables_semantically() {
    let left = external("hdiffpatch/windows64/hdiffz.exe");
    let right = external("hdiffpatch/windows64/hpatchz.exe");
    if !left.exists() || !right.exists() {
        return;
    }
    let report = engine().compare(&left, &right).unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "pe");
            assert!(!diff.changes.is_empty());
            assert!(
                diff.changes
                    .iter()
                    .any(|change| change.path.starts_with("$.sections")),
                "{:?}",
                diff.changes
            );
        }
        other => panic!("expected structured pe diff, got {other:?}"),
    }
}

#[test]
fn compares_sqlite_databases_semantically() {
    let report = engine()
        .compare(&fixture("sqlite/old.db"), &fixture("sqlite/new.db"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "sqlite");
            assert_eq!(diff.changes.len(), 2, "{:?}", diff.changes);
            assert!(diff.changes.iter().any(|change| {
                change.path == "$.tables.users.rows[0][2]"
                    && change.left == Some(Value::from(30))
                    && change.right == Some(Value::from(31))
            }));
            assert!(diff.changes.iter().any(|change| {
                change.path == "$.tables.users.rows[2]"
                    && change.right.as_ref().is_some_and(|value| value[0] == 3)
            }));
        }
        other => panic!("expected structured sqlite diff, got {other:?}"),
    }

    let equal = engine()
        .compare(&fixture("sqlite/old.db"), &fixture("sqlite/old-copy.db"))
        .unwrap();
    assert_eq!(equal.status, DiffStatus::Equal);
    match &equal.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "sqlite");
            assert!(diff.changes.is_empty());
        }
        other => panic!("expected structured sqlite diff, got {other:?}"),
    }
}

#[test]
fn compares_yaml_toml_ini_and_xml_semantically() {
    for (left, right, format) in [
        ("structured/old.yaml", "structured/new.yaml", "yaml"),
        ("structured/old.toml", "structured/new.toml", "toml"),
        ("structured/old.ini", "structured/new.ini", "ini"),
        ("structured/old.xml", "structured/new.xml", "xml"),
    ] {
        let report = engine().compare(&fixture(left), &fixture(right)).unwrap();
        assert_eq!(report.status, DiffStatus::Different, "{format}");
        assert!(report.summary.structured_changes > 0, "{format}");
        match &report.root.body {
            DiffBody::Structured(diff) => assert_eq!(diff.format, format),
            other => panic!("expected structured diff for {format}, got {other:?}"),
        }
    }
}

#[test]
fn compares_xml_and_binary_plists() {
    let report = engine()
        .compare(&fixture("plist/old.plist"), &fixture("plist/new.plist"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "plist");
            assert!(
                diff.changes
                    .iter()
                    .any(|change| change.path == "$.CFBundleVersion")
            );
            assert!(
                diff.changes
                    .iter()
                    .any(|change| change.path == "$.Settings.Theme")
            );
        }
        other => panic!("expected plist structured diff, got {other:?}"),
    }

    let binary = engine()
        .compare(
            &fixture("plist/old-binary.plist"),
            &fixture("plist/new-binary.plist"),
        )
        .unwrap();
    assert_eq!(binary.status, DiffStatus::Different);
    assert!(matches!(binary.root.body, DiffBody::Structured(_)));

    let cross_encoding = engine()
        .compare(
            &fixture("plist/old.plist"),
            &fixture("plist/old-binary.plist"),
        )
        .unwrap();
    assert_eq!(cross_encoding.status, DiffStatus::Equal);

    let cross_format = engine()
        .compare(&fixture("plist/old.plist"), &fixture("plist/old.json"))
        .unwrap();
    assert_eq!(cross_format.status, DiffStatus::Equal);
    match &cross_format.root.body {
        DiffBody::Structured(diff) => assert_eq!(diff.format, "plist+json"),
        other => panic!("expected cross-format plist diff, got {other:?}"),
    }
}

#[test]
fn compares_pickles_and_cross_encoding() {
    let report = engine()
        .compare(&fixture("pickle/old.pkl"), &fixture("pickle/new.pkl"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "pickle");
            assert!(diff.changes.iter().any(|change| change.path == "$.version"));
            assert!(
                diff.changes
                    .iter()
                    .any(|change| change.path == "$.items[0].tags[1]")
            );
        }
        other => panic!("expected pickle structured diff, got {other:?}"),
    }

    let cross_protocol = engine()
        .compare(
            &fixture("pickle/old.pkl"),
            &fixture("pickle/old-protocol5.pkl"),
        )
        .unwrap();
    assert_eq!(cross_protocol.status, DiffStatus::Equal);

    let cross_format = engine()
        .compare(&fixture("pickle/old.pkl"), &fixture("pickle/old.json"))
        .unwrap();
    assert_eq!(cross_format.status, DiffStatus::Equal);
    match &cross_format.root.body {
        DiffBody::Structured(diff) => assert_eq!(diff.format, "pickle+json"),
        other => panic!("expected cross-format pickle diff, got {other:?}"),
    }
}

#[test]
fn compares_openapi_breaking_changes() {
    let report = engine()
        .compare(&fixture("openapi/old.yaml"), &fixture("openapi/new.yaml"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "openapi");
            assert!(
                diff.changes
                    .iter()
                    .any(|change| { change.path == "$.paths./pets.post" && change.breaking }),
                "{:?}",
                diff.changes
            );
            assert!(
                diff.changes.iter().any(|change| {
                    change.path == "$.paths./pets.get.parameters" && change.breaking
                }),
                "{:?}",
                diff.changes
            );
            for path in [
                "$.paths./owners.post.requestBody",
                "$.components.schemas.Pet.properties.id.type",
                "$.components.schemas.Pet.required[2]",
                "$.components.schemas.Pet.properties.tag",
            ] {
                assert!(
                    diff.changes
                        .iter()
                        .any(|change| change.path == path && change.breaking),
                    "missing breaking change {path}: {:?}",
                    diff.changes
                );
            }
            assert!(
                diff.changes
                    .iter()
                    .any(|change| { change.path == "$.info.version" && !change.breaking }),
                "{:?}",
                diff.changes
            );
        }
        other => panic!("expected OpenAPI structured diff, got {other:?}"),
    }

    let cross_format = engine()
        .compare(&fixture("openapi/old.yaml"), &fixture("openapi/old.json"))
        .unwrap();
    assert_eq!(cross_format.status, DiffStatus::Equal);
    match &cross_format.root.body {
        DiffBody::Structured(diff) => assert_eq!(diff.format, "openapi"),
        other => panic!("expected OpenAPI cross-format diff, got {other:?}"),
    }
}

#[test]
fn compares_java_class_files() {
    let report = engine()
        .compare(&fixture("java/Test1.class"), &fixture("java/Test2.class"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "java-class");
            assert!(diff.changes.iter().any(|change| {
                change.path == "$.methods[1].code.digest"
                    && change.left.is_some()
                    && change.right.is_some()
            }));
        }
        other => panic!("expected structured java class diff, got {other:?}"),
    }

    let equal = engine()
        .compare(&fixture("java/Test1.class"), &fixture("java/Test1.class"))
        .unwrap();
    assert_eq!(equal.status, DiffStatus::Equal);
}

#[test]
fn compares_openssh_public_keys() {
    let report = engine()
        .compare(
            &fixture("ssh/test_openssh_pub_key1.pub"),
            &fixture("ssh/test_openssh_pub_key2.pub"),
        )
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "openssh-key");
            assert!(diff.changes.iter().any(|change| {
                change.path == "$.key_type"
                    && change.left == Some(Value::from("ssh-dss"))
                    && change.right == Some(Value::from("ssh-rsa"))
            }));
            assert!(diff.changes.iter().any(|change| change.path == "$.bits"));
        }
        other => panic!("expected structured openssh diff, got {other:?}"),
    }

    let equal = engine()
        .compare(
            &fixture("ssh/test_openssh_pub_key1.pub"),
            &fixture("ssh/test_openssh_pub_key1.pub"),
        )
        .unwrap();
    assert_eq!(equal.status, DiffStatus::Equal);
}

#[test]
fn compares_ipk_packages_recursively() {
    let report = engine()
        .compare(&fixture("ipk/old.ipk"), &fixture("ipk/new.ipk"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Archive(diff) => {
            assert_eq!(diff.format, "tar");
            assert_eq!(diff.total_entries, 3);
            let control = diff
                .entries
                .iter()
                .find(|entry| entry.label == "control.tar.gz")
                .expect("control.tar.gz entry");
            assert_eq!(control.status, DiffStatus::Different);
            assert!(matches!(control.body, DiffBody::Archive(_)));
            let data = diff
                .entries
                .iter()
                .find(|entry| entry.label == "data.tar.gz")
                .expect("data.tar.gz entry");
            assert_eq!(data.status, DiffStatus::Different);
            assert!(matches!(data.body, DiffBody::Archive(_)));
        }
        other => panic!("expected tar archive diff, got {other:?}"),
    }

    let equal = engine()
        .compare(&fixture("ipk/old.ipk"), &fixture("ipk/old.ipk"))
        .unwrap();
    assert_eq!(equal.status, DiffStatus::Equal);
}

#[test]
fn compares_ico_images() {
    let report = engine()
        .compare(
            &fixture("images/diffoscope-test1.ico"),
            &fixture("images/diffoscope-test2.ico"),
        )
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "ico");
            assert!(diff.changes.iter().any(|change| {
                change.path == "$.images[0].dib_bit_count"
                    && change.left == Some(Value::from(4))
                    && change.right == Some(Value::from(32))
            }));
        }
        other => panic!("expected structured ico diff, got {other:?}"),
    }

    let equal = engine()
        .compare(
            &fixture("images/diffoscope-test1.ico"),
            &fixture("images/diffoscope-test1.ico"),
        )
        .unwrap();
    assert_eq!(equal.status, DiffStatus::Equal);
}

#[test]
fn compares_git_indexes() {
    let report = engine()
        .compare(
            &fixture("git/test1.git-index"),
            &fixture("git/test2.git-index"),
        )
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "git-index");
            assert!(diff.changes.iter().any(|change| {
                change.path.starts_with("$.entries")
                    && change
                        .right
                        .as_ref()
                        .is_some_and(|value| value["path"] == "b")
            }));
        }
        other => panic!("expected structured git index diff, got {other:?}"),
    }

    let equal = engine()
        .compare(
            &fixture("git/test1.git-index"),
            &fixture("git/test1.git-index"),
        )
        .unwrap();
    assert_eq!(equal.status, DiffStatus::Equal);
}

#[test]
fn compares_apk_dex_decompiled_code() {
    let options = DiffOptions {
        decompile: Some(diff_core::engine::DecompileOptions::default()),
        ..DiffOptions::default()
    };
    let engine = DiffEngine::new(options);
    let report = engine
        .compare(&fixture("apk/code-old.apk"), &fixture("apk/code-new.apk"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Archive(diff) => {
            let dex = diff
                .entries
                .iter()
                .find(|entry| entry.label == "classes.dex")
                .expect("classes.dex entry");
            match &dex.body {
                DiffBody::Text(text) => {
                    assert_eq!(text.format, "dex-java");
                    assert!(
                        text.hunks
                            .iter()
                            .flat_map(|hunk| &hunk.lines)
                            .any(|line| line.text.contains("onCreate")),
                        "{:?}",
                        text.hunks
                    );
                }
                other => panic!("expected decompiled DEX text, got {other:?}"),
            }
        }
        other => panic!("expected zip archive diff, got {other:?}"),
    }
}

#[test]
fn compares_apk_manifests_semantically() {
    let report = engine()
        .compare(&fixture("apk/old.apk"), &fixture("apk/new.apk"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Archive(diff) => {
            assert_eq!(diff.format, "zip");
            let manifest = diff
                .entries
                .iter()
                .find(|entry| entry.label == "AndroidManifest.xml")
                .expect("AndroidManifest.xml entry");
            match &manifest.body {
                DiffBody::Structured(structured) => {
                    assert_eq!(structured.format, "android-xml");
                    assert!(structured.changes.iter().any(|change| {
                        change.path == "$.root.attributes.android:versionCode"
                            && change.left == Some(Value::from("1"))
                            && change.right == Some(Value::from("2"))
                    }));
                    assert!(
                        structured.changes.iter().any(|change| {
                            change.path == "$.root.attributes.android:versionName"
                        })
                    );
                }
                other => panic!("expected nested android-xml diff, got {other:?}"),
            }
        }
        other => panic!("expected zip archive diff, got {other:?}"),
    }
}

#[test]
fn compares_jar_java_class_semantically() {
    let report = engine()
        .compare(&fixture("java/old.jar"), &fixture("java/new.jar"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Archive(diff) => {
            assert_eq!(diff.format, "zip");
            let class = diff
                .entries
                .iter()
                .find(|entry| entry.label == "Test.class")
                .expect("Test.class entry");
            assert_eq!(class.status, DiffStatus::Different);
            match &class.body {
                DiffBody::Structured(structured) => assert_eq!(structured.format, "java-class"),
                other => panic!("expected nested java-class diff, got {other:?}"),
            }
        }
        other => panic!("expected zip archive diff, got {other:?}"),
    }
}

#[test]
fn compares_wasm_modules() {
    let report = engine()
        .compare(&fixture("wasm/old.wasm"), &fixture("wasm/new.wasm"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "wasm");
            assert!(diff.changes.iter().any(|change| {
                change.path.starts_with("$.exports")
                    && change
                        .right
                        .as_ref()
                        .is_some_and(|value| value["name"] == "mul")
            }));
            assert!(
                diff.changes
                    .iter()
                    .any(|change| change.path.starts_with("$.code")),
                "{:?}",
                diff.changes
            );
        }
        other => panic!("expected structured wasm diff, got {other:?}"),
    }

    let equal = engine()
        .compare(&fixture("wasm/old.wasm"), &fixture("wasm/old.wasm"))
        .unwrap();
    assert_eq!(equal.status, DiffStatus::Equal);
}

#[test]
fn compares_gettext_catalogs() {
    let report = engine()
        .compare(&fixture("gettext/test1.mo"), &fixture("gettext/test2.mo"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "gettext");
            assert!(diff.changes.iter().any(|change| {
                change.path == "$.headers.PO-Revision-Date"
                    && change.left.is_some()
                    && change.right.is_some()
            }));
            assert!(diff.changes.iter().any(|change| {
                change.path.starts_with("$.messages[") && change.kind == StructuredChangeKind::Added
            }));
        }
        other => panic!("expected structured gettext diff, got {other:?}"),
    }

    let equal = engine()
        .compare(&fixture("gettext/test1.mo"), &fixture("gettext/test1.mo"))
        .unwrap();
    assert_eq!(equal.status, DiffStatus::Equal);
}

#[test]
fn compares_gettext_plural_forms() {
    let report = engine()
        .compare(
            &fixture("gettext/plural-old.mo"),
            &fixture("gettext/plural-new.mo"),
        )
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "gettext");
            assert!(diff.changes.iter().any(|change| {
                change.path == "$.messages[0].msgstr[1]"
                    && change.left == Some(Value::from("fichiers"))
                    && change.right == Some(Value::from("documents"))
            }));
        }
        other => panic!("expected structured gettext diff, got {other:?}"),
    }
}

#[test]
fn compares_pickles_with_unknown_globals_readably() {
    let report = engine()
        .compare(
            &fixture("pickle/old-global.pkl"),
            &fixture("pickle/new-global.pkl"),
        )
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "pickle");
            assert!(
                diff.changes.iter().any(|change| {
                    change.path == "$.$readable_strings[0]"
                        && change.left.as_ref().and_then(|value| value.as_str()) == Some("Widget")
                        && change.right.as_ref().and_then(|value| value.as_str()) == Some("Gadget")
                }),
                "{:?}",
                diff.changes
            );
        }
        other => panic!("expected readable pickle diff, got {other:?}"),
    }
}

#[test]
fn invalid_pickles_fall_back_to_readable_objects() {
    let dir = tempfile::tempdir().unwrap();
    let left = dir.path().join("left.pkl");
    let right = dir.path().join("right.pkl");
    std::fs::write(&left, [0x80, 0x04, 0xff, b'n', b'o', b't', b' ']).unwrap();
    std::fs::write(&right, [0x80, 0x05, 0xfe, b'b', b'a', b'd', b' ']).unwrap();
    let report = engine().compare(&left, &right).unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "pickle");
            assert!(
                diff.changes
                    .iter()
                    .any(|change| change.path == "$.$bytes_hash"),
                "{:?}",
                diff.changes
            );
        }
        other => panic!("expected readable invalid-pickle diff, got {other:?}"),
    }
}

#[test]
fn compares_html_semantically() {
    let report = engine()
        .compare(
            &fixture("structured/old.html"),
            &fixture("structured/new.html"),
        )
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "html");
            assert!(!diff.changes.is_empty());
        }
        other => panic!("expected HTML structured diff, got {other:?}"),
    }
}

#[test]
fn compares_json_against_yaml() {
    let report = engine()
        .compare(
            &fixture("structured/cross.yaml"),
            &fixture("structured/cross.json"),
        )
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Structured(diff) => {
            assert_eq!(diff.format, "yaml+json");
            assert_eq!(diff.changes.len(), 1);
            assert_eq!(diff.changes[0].path, "$.settings.theme");
        }
        other => panic!("expected cross-format structured diff, got {other:?}"),
    }
}

#[test]
fn compares_csv_with_key_column() {
    let options = DiffOptions {
        key_column: Some("id".to_string()),
        ..DiffOptions::default()
    };
    let report = DiffEngine::new(options)
        .compare(&fixture("table/old.csv"), &fixture("table/new.csv"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert_eq!(report.summary.table_rows_added, 1);
    assert_eq!(report.summary.table_rows_removed, 1);
    assert_eq!(report.summary.table_rows_changed, 1);
    match &report.root.body {
        DiffBody::Table(diff) => {
            assert!(
                diff.rows
                    .iter()
                    .any(|row| row.key == "1" && row.kind == RowChangeKind::Changed)
            );
            assert!(
                diff.rows
                    .iter()
                    .any(|row| row.key == "3" && row.kind == RowChangeKind::Removed)
            );
            assert!(
                diff.rows
                    .iter()
                    .any(|row| row.key == "4" && row.kind == RowChangeKind::Added)
            );
        }
        other => panic!("expected table diff, got {other:?}"),
    }
}

#[test]
fn compares_tsv_tables() {
    let options = DiffOptions {
        key_column: Some("id".to_string()),
        ..DiffOptions::default()
    };
    let report = DiffEngine::new(options)
        .compare(&fixture("table/old.tsv"), &fixture("table/new.tsv"))
        .unwrap();
    match &report.root.body {
        DiffBody::Table(diff) => {
            assert_eq!(diff.delimiter, '\t');
            assert_eq!(diff.format, "tsv");
            assert_eq!(diff.rows.len(), 2);
        }
        other => panic!("expected TSV table diff, got {other:?}"),
    }
}

#[test]
fn compares_docx_and_pptx_text() {
    for (left, right, format) in [
        ("docs/old.docx", "docs/new.docx", "docx"),
        ("docs/old.pptx", "docs/new.pptx", "pptx"),
    ] {
        let report = engine().compare(&fixture(left), &fixture(right)).unwrap();
        assert_eq!(report.status, DiffStatus::Different);
        match &report.root.body {
            DiffBody::Document(diff) => assert_eq!(diff.format, format),
            DiffBody::Presentation(diff) => assert_eq!(diff.format, format),
            DiffBody::Text(diff) => assert_eq!(diff.format, format),
            other => panic!("expected {format} structured diff, got {other:?}"),
        }
    }
}

#[test]
fn cross_format_docx_and_odt_use_block_mapping() {
    let report = engine()
        .compare(&fixture("docs/old.docx"), &fixture("documents/old.odt"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Document(diff) => {
            assert_eq!(diff.format, "docx-to-odt");
            assert!(
                diff.blocks
                    .iter()
                    .any(|block| block.path.starts_with("body/p["))
            );
        }
        other => panic!("expected cross-format document diff, got {other:?}"),
    }
}

#[test]
fn cross_format_pdf_and_docx_use_block_mapping() {
    let report = engine()
        .compare(&fixture("pdf/old.pdf"), &fixture("docs/old.docx"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Document(diff) => {
            assert_eq!(diff.format, "pdf-to-docx");
            assert!(
                diff.blocks
                    .iter()
                    .any(|block| block.path.starts_with("page[") || block.path.starts_with("body/"))
            );
        }
        other => panic!("expected cross-format document diff, got {other:?}"),
    }
}

#[test]
fn cross_format_pptx_and_odp_use_block_mapping() {
    let report = engine()
        .compare(&fixture("docs/old.pptx"), &fixture("documents/old.odp"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Document(diff) => {
            assert_eq!(diff.format, "pptx-to-odp");
            assert!(
                diff.blocks
                    .iter()
                    .any(|block| block.path.starts_with("slide["))
            );
        }
        other => panic!("expected cross-format presentation diff, got {other:?}"),
    }
}

#[test]
fn compares_ofd_text() {
    let report = engine()
        .compare(&fixture("ofd/old.ofd"), &fixture("ofd/new.ofd"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Document(diff) => {
            assert_eq!(diff.format, "ofd");
            assert!(diff.left_pages.unwrap_or(0) > 0);
            assert!(!diff.blocks.is_empty());
        }
        other => panic!("expected OFD document diff, got {other:?}"),
    }
}

#[test]
fn compares_pdf_text() {
    let report = engine()
        .compare(&fixture("pdf/old.pdf"), &fixture("pdf/new.pdf"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Document(diff) => {
            assert_eq!(diff.format, "pdf");
            assert_eq!(diff.right_pages, Some(14));
            assert!(diff.left_pages.unwrap_or(0) > 0);
            assert!(!diff.blocks.is_empty());
            assert!(
                diff.blocks
                    .iter()
                    .any(|block| block.path.starts_with("page["))
            );
            assert!(
                diff.blocks
                    .iter()
                    .any(|block| block.path.contains("/block["))
            );
            assert!(
                diff.blocks.iter().any(|block| block.position.is_some()),
                "{:?}",
                diff.blocks
            );
            assert!(
                diff.metadata
                    .iter()
                    .any(|field| field.path.starts_with("metadata/page[1]/size")),
                "{:?}",
                diff.metadata
            );
        }
        other => panic!("expected PDF document diff, got {other:?}"),
    }
}

#[test]
fn compares_workbooks() {
    let report = engine()
        .compare(&fixture("workbook/old.xlsx"), &fixture("workbook/new.xlsx"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Workbook(diff) => assert_eq!(diff.format, "xlsx"),
        other => panic!("expected workbook diff, got {other:?}"),
    }
}

#[test]
fn compares_workbook_cells() {
    use rust_xlsxwriter::Workbook;

    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old.xlsx");
    let new = dir.path().join("new.xlsx");

    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.write(0, 0, "Name").unwrap();
    sheet.write(0, 1, "Value").unwrap();
    sheet.write(1, 0, "Alice").unwrap();
    sheet.write(1, 1, 1).unwrap();
    sheet.write(2, 0, "Bob").unwrap();
    sheet.write(2, 1, 2).unwrap();
    workbook.save(&old).unwrap();

    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.write(0, 0, "Name").unwrap();
    sheet.write(0, 1, "Value").unwrap();
    sheet.write(1, 0, "Alice").unwrap();
    sheet.write(1, 1, 1).unwrap();
    sheet.write(2, 0, "Bob").unwrap();
    sheet.write(2, 1, 3).unwrap();
    sheet.write(3, 0, "Carol").unwrap();
    sheet.write(3, 1, 4).unwrap();
    workbook.save(&new).unwrap();

    let report = DiffEngine::new(DiffOptions::default())
        .compare(&old, &new)
        .unwrap();
    assert!(report.summary.workbook_cells_changed >= 3);
    match report.root.body {
        DiffBody::Workbook(diff) => {
            let sheet = &diff.sheets[0];
            assert!(
                sheet
                    .cells
                    .iter()
                    .any(|cell| cell.column == "B3" && cell.right.as_deref() == Some("3"))
            );
            assert!(sheet.cells.iter().any(|cell| cell.column == "A4"));
        }
        other => panic!("expected workbook diff, got {other:?}"),
    }
}

#[test]
fn reads_legacy_xls_and_ods() {
    let report = engine()
        .compare(&fixture("workbook/old.xls"), &fixture("workbook/old.ods"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Workbook(diff) => {
            assert!(diff.format == "xls" || diff.format == "ods");
        }
        other => panic!("expected workbook diff, got {other:?}"),
    }
}

#[test]
fn compares_tar_symlinks_and_hardlinks() {
    let report = engine()
        .compare(
            &fixture("archives/links-old.tar"),
            &fixture("archives/links-new.tar"),
        )
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Archive(diff) => {
            assert_eq!(diff.format, "tar");
            assert_eq!(diff.total_entries, 5);
            assert!(diff.entries.iter().any(|entry| {
                entry.label == "target.txt" && entry.status == DiffStatus::Different
            }));
            assert!(
                diff.entries.iter().any(|entry| {
                    entry.label == "same.txt" && entry.status == DiffStatus::Equal
                })
            );
            assert!(
                diff.entries.iter().any(|entry| {
                    entry.label == "hard.txt" && entry.status == DiffStatus::Equal
                })
            );
        }
        other => panic!("expected tar archive diff, got {other:?}"),
    }
}

#[test]
fn compares_tar_zst_archives() {
    let report = engine()
        .compare(
            &fixture("archive-zst/old.tar.zst"),
            &fixture("archive-zst/new.tar.zst"),
        )
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Archive(diff) => {
            assert_eq!(diff.format, "tar.zst");
            assert!(diff.entries.iter().any(|entry| entry.label == "extra.txt"));
        }
        other => panic!("expected tar.zst archive diff, got {other:?}"),
    }

    let cross_compression = engine()
        .compare(
            &fixture("archive-zst/old.tar.zst"),
            &fixture("archive-zst/old.tar.gz"),
        )
        .unwrap();
    assert_eq!(cross_compression.status, DiffStatus::Equal);

    let single = engine()
        .compare(
            &fixture("archive-zst/old.log.zst"),
            &fixture("archive-zst/new.log.zst"),
        )
        .unwrap();
    assert_eq!(single.status, DiffStatus::Different);
    match &single.root.body {
        DiffBody::Text(diff) => assert_eq!(diff.added_lines, 2),
        other => panic!("expected decompressed zstd text diff, got {other:?}"),
    }
}

#[test]
fn compares_deb_packages_recursively() {
    let report = engine()
        .compare(&fixture("deb/old.deb"), &fixture("deb/new.deb"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Archive(diff) => {
            assert_eq!(diff.format, "deb");
            assert_eq!(diff.total_entries, 3);
            assert!(
                diff.entries
                    .iter()
                    .any(|entry| entry.label == "data.tar.gz"
                        && entry.status == DiffStatus::Different)
            );
            assert!(
                diff.entries
                    .iter()
                    .any(|entry| entry.label == "control.tar.gz"
                        && entry.status == DiffStatus::Different)
            );
        }
        other => panic!("expected deb archive diff, got {other:?}"),
    }
    assert!(report.summary.files_changed >= 2);

    let equal = engine()
        .compare(&fixture("deb/old.deb"), &fixture("deb/old.deb"))
        .unwrap();
    assert_eq!(equal.status, DiffStatus::Equal);
}

#[test]
fn compares_single_bzip2_and_xz_files() {
    for (left, right, format) in [
        (
            "compression/old.txt.bz2",
            "compression/new.txt.bz2",
            "bzip2",
        ),
        ("compression/old.txt.xz", "compression/new.txt.xz", "xz"),
    ] {
        let report = engine().compare(&fixture(left), &fixture(right)).unwrap();
        assert_eq!(report.status, DiffStatus::Different, "{format}");
        match &report.root.body {
            DiffBody::Text(diff) => {
                assert_eq!(diff.added_lines, 2, "{format}");
                assert_eq!(diff.removed_lines, 2, "{format}");
            }
            other => panic!("expected decompressed {format} text diff, got {other:?}"),
        }
    }
}

#[test]
fn compares_real_rpm_packages() {
    let left = external("hello-2.12.1-2.fc39.x86_64.rpm");
    let right = external("hello-2.12.1-4.fc40.x86_64.rpm");
    if !left.is_file() || !right.is_file() {
        return;
    }
    let report = engine().compare(&left, &right).unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Archive(diff) => {
            assert_eq!(diff.format, "rpm");
            assert!(diff.total_entries > 50);
            assert!(diff.entries.iter().any(|entry| {
                entry.label == ".rpm-metadata.json" && entry.status == DiffStatus::Different
            }));
        }
        other => panic!("expected rpm archive diff, got {other:?}"),
    }
    assert!(report.summary.structured_changes > 0);
}

#[test]
fn compares_real_xz_payload_rpm_packages() {
    let left = external("which-2.21-12.el8.x86_64.rpm");
    let right = external("which-2.21-16.el8.x86_64.rpm");
    if !left.is_file() || !right.is_file() {
        return;
    }
    let bytes = std::fs::read(&left).unwrap();
    assert!(
        bytes.windows(6).any(|window| window == b"\xfd7zXZ\0"),
        "fixture should contain an xz payload"
    );

    let report = engine().compare(&left, &right).unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Archive(diff) => {
            assert_eq!(diff.format, "rpm");
            assert!(diff.total_entries > 10);
            assert!(diff.entries.iter().any(|entry| {
                entry.label == ".rpm-metadata.json" && entry.status == DiffStatus::Different
            }));
        }
        other => panic!("expected xz-payload rpm archive diff, got {other:?}"),
    }
    assert!(
        !report
            .warnings
            .iter()
            .any(|warning| warning.contains("falling back to binary")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn compares_cpio_archives() {
    for (left, right) in [
        ("cpio/old.cpio", "cpio/new.cpio"),
        ("cpio/old.cpio.gz", "cpio/new.cpio.gz"),
    ] {
        let report = engine().compare(&fixture(left), &fixture(right)).unwrap();
        assert_eq!(report.status, DiffStatus::Different, "{left}");
        match &report.root.body {
            DiffBody::Archive(diff) => {
                assert_eq!(diff.format, "cpio");
                assert_eq!(diff.total_entries, 3);
                assert!(
                    diff.entries
                        .iter()
                        .any(|entry| entry.label == "usr/share/demo/config.json")
                );
            }
            other => panic!("expected cpio archive diff, got {other:?}"),
        }
    }
}

#[test]
fn compares_zip_bzip2_lzma_and_zstd_entries() {
    for method in ["bz2", "lzma", "zstd"] {
        let left = fixture(&format!("zip-methods/old-{method}.zip"));
        let right = fixture(&format!("zip-methods/new-{method}.zip"));
        let report = engine().compare(&left, &right).unwrap();
        assert_eq!(report.status, DiffStatus::Different, "{method}");
        match &report.root.body {
            DiffBody::Archive(diff) => {
                assert_eq!(diff.format, "zip", "{method}");
                assert_eq!(diff.total_entries, 2, "{method}");
                assert!(
                    diff.entries.iter().any(|entry| entry.label == "a.txt"),
                    "{method}"
                );
            }
            other => panic!("expected zip archive diff for {method}, got {other:?}"),
        }
    }
}

#[test]
fn compares_lz4_archives_and_single_files() {
    let report = engine()
        .compare(&fixture("lz4/old.tar.lz4"), &fixture("lz4/new.tar.lz4"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Archive(diff) => {
            assert_eq!(diff.format, "tar.lz4");
            assert!(diff.entries.iter().any(|entry| entry.label == "extra.txt"));
        }
        other => panic!("expected tar.lz4 archive diff, got {other:?}"),
    }

    let single = engine()
        .compare(&fixture("lz4/old.log.lz4"), &fixture("lz4/new.log.lz4"))
        .unwrap();
    assert_eq!(single.status, DiffStatus::Different);
    assert!(matches!(single.root.body, DiffBody::Text(_)));
}

#[test]
fn compares_iso_images() {
    let report = engine()
        .compare(&fixture("iso/old.iso"), &fixture("iso/new.iso"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match &report.root.body {
        DiffBody::Archive(diff) => {
            assert_eq!(diff.format, "iso");
            assert_eq!(diff.total_entries, 4);
            assert!(diff.entries.iter().any(|entry| entry.label == "EXTRA.TXT"));
            assert!(
                diff.entries
                    .iter()
                    .any(|entry| entry.label == "SRC/MAIN.RS")
            );
        }
        other => panic!("expected iso archive diff, got {other:?}"),
    }

    let equal = engine()
        .compare(&fixture("iso/old.iso"), &fixture("iso/old.iso"))
        .unwrap();
    assert_eq!(equal.status, DiffStatus::Equal);
}

#[test]
fn compares_tar_gz_and_zip_archives() {
    for (left, right, format) in [
        ("archives/old.tar.gz", "archives/new.tar.gz", "tar.gz"),
        ("archives/old.tar.bz2", "archives/new.tar.bz2", "tar.bz2"),
        ("archives/old.tar.xz", "archives/new.tar.xz", "tar.xz"),
        ("archives/old.7z", "archives/new.7z", "7z"),
        ("zip/old.zip", "zip/new.zip", "zip"),
    ] {
        let report = engine().compare(&fixture(left), &fixture(right)).unwrap();
        assert_eq!(report.status, DiffStatus::Different);
        match &report.root.body {
            DiffBody::Archive(diff) => {
                assert_eq!(diff.format, format);
                assert!(diff.total_entries > 0);
            }
            other => panic!("expected {format} archive diff, got {other:?}"),
        }
    }
}

#[test]
fn moved_notebook_cells_are_different_without_add_remove() {
    let dir = tempfile::tempdir().unwrap();
    let left_path = dir.path().join("left.ipynb");
    let right_path = dir.path().join("right.ipynb");
    std::fs::write(
        &left_path,
        serde_json::to_vec(&notebook_json(vec![
            notebook_cell("a", "markdown", "a\n"),
            notebook_cell("b", "markdown", "b\n"),
            notebook_cell("c", "markdown", "c\n"),
        ]))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        &right_path,
        serde_json::to_vec(&notebook_json(vec![
            notebook_cell("c", "markdown", "c\n"),
            notebook_cell("a", "markdown", "a\n"),
            notebook_cell("b", "markdown", "b\n"),
        ]))
        .unwrap(),
    )
    .unwrap();

    let report = engine().compare(&left_path, &right_path).unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert_eq!(report.summary.notebook_cells_added, 0);
    assert_eq!(report.summary.notebook_cells_removed, 0);
    assert_eq!(report.summary.notebook_cells_moved, 1);
}

#[test]
fn compares_notebook_json() {
    let report = engine()
        .compare(
            &fixture("notebook/old.ipynb"),
            &fixture("notebook/new.ipynb"),
        )
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert!(report.summary.notebook_cells_changed > 0);
    match &report.root.body {
        DiffBody::Notebook(diff) => {
            assert_eq!(diff.left_cells, 2);
            assert_eq!(diff.right_cells, 2);
            assert!(!diff.changed_cells.is_empty());
        }
        other => panic!("expected notebook cell diff, got {other:?}"),
    }
}

#[test]
fn compares_audio_metadata() {
    let report = engine()
        .compare(&fixture("audio/old.wav"), &fixture("audio/new.wav"))
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert!(report.summary.audio_property_changes > 0);
    match &report.root.body {
        DiffBody::Audio(diff) => {
            assert_eq!(diff.left_duration_ms, 250);
            assert_eq!(diff.right_duration_ms, 500);
            assert_eq!(diff.left_sample_rate, Some(8000));
            assert_eq!(diff.right_sample_rate, Some(16000));
            assert_eq!(diff.left_channels, Some(1));
            assert_eq!(diff.right_channels, Some(2));
            assert!(diff.content_differs);
        }
        other => panic!("expected audio diff, got {other:?}"),
    }
}

#[test]
fn compares_audio_tags() {
    use lofty::config::WriteOptions;
    use lofty::prelude::{Accessor, AudioFile, TaggedFileExt};
    use lofty::tag::{Tag, TagType};

    let dir = tempfile::tempdir().unwrap();
    let left = dir.path().join("left.wav");
    let right = dir.path().join("right.wav");
    std::fs::copy(fixture("audio/old.wav"), &left).unwrap();
    std::fs::copy(fixture("audio/new.wav"), &right).unwrap();

    for (path, title, artist) in [(&left, "Old title", "Alice"), (&right, "New title", "Bob")] {
        let mut file = lofty::read_from_path(path).unwrap();
        let mut tag = Tag::new(TagType::Id3v2);
        tag.set_title(title.to_string());
        tag.set_artist(artist.to_string());
        file.insert_tag(tag);
        file.save_to_path(path, WriteOptions::default()).unwrap();
    }

    let report = DiffEngine::new(DiffOptions::default())
        .compare(&left, &right)
        .unwrap();
    match report.root.body {
        DiffBody::Audio(diff) => {
            assert!(diff.tags.iter().any(|tag| tag.key.contains("Title")));
            assert!(diff.tags.iter().any(|tag| tag.key.contains("Artist")));
        }
        other => panic!("expected audio diff, got {other:?}"),
    }
}

#[test]
fn compares_images_and_binaries() {
    let image_report = engine()
        .compare(&fixture("images/old.png"), &fixture("images/new.png"))
        .unwrap();
    match &image_report.root.body {
        DiffBody::Image(diff) => assert!(diff.differing_pixels > 0),
        other => panic!("expected image diff, got {other:?}"),
    }

    let binary_report = engine()
        .compare(&fixture("binary/old.bin"), &fixture("binary/new.bin"))
        .unwrap();
    match &binary_report.root.body {
        DiffBody::Binary(diff) => {
            assert_eq!(diff.differing_bytes, 3);
            assert_eq!(diff.first_difference, Some(10));
        }
        other => panic!("expected binary diff, got {other:?}"),
    }
}

#[test]
fn excludes_paths_in_directory_comparison() {
    let options = DiffOptions {
        excludes: vec!["*.txt".to_string()],
        ..DiffOptions::default()
    };
    let report = DiffEngine::new(options)
        .compare(&fixture("dir/left"), &fixture("dir/right"))
        .unwrap();
    match &report.root.body {
        DiffBody::Directory(directory) => {
            assert!(
                directory
                    .entries
                    .iter()
                    .all(|entry| !entry.label.ends_with(".txt"))
            );
        }
        other => panic!("expected directory diff, got {other:?}"),
    }
}
