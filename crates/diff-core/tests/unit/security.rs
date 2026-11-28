//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn parses_clean_and_threat_reports() {
    let clean = parse_report("Scan completed. Threats detected: 0");
    assert!(clean.threats.is_empty());

    let threats = parse_report("Detected: Trojan.Win32.Test C:\\samples\\bad.exe\nScan completed");
    assert_eq!(threats.threats.len(), 1);
    assert!(threats.threats[0].contains("bad.exe"));
}

#[test]
fn explicit_installation_is_used() {
    let path = std::env::current_exe().unwrap();
    let installation = detect_kaspersky(Some(&path)).unwrap();
    assert_eq!(installation.executable, path);
    assert!(installation.service_running.is_none());
}

#[test]
fn scans_with_mock_kaspersky_command() {
    let directory = tempfile::tempdir().unwrap();
    let sample = directory.path().join("sample.bin");
    std::fs::write(&sample, b"not malware").unwrap();

    #[cfg(windows)]
    let script = {
        let script = directory.path().join("mock-avp.cmd");
        std::fs::write(
            &script,
            b"@echo off\r\nsetlocal enabledelayedexpansion\r\nset REPORT=\r\nfor %%A in (%*) do (\r\n  set ARG=%%A\r\n  if \"!ARG:~0,3!\"==\"/R:\" set REPORT=!ARG:~3!\r\n)\r\necho Detected: Trojan.Win32.Mock C:\\sample.bin > \"%REPORT%\"\r\nexit /b 0\r\n",
        )
        .unwrap();
        script
    };
    #[cfg(not(windows))]
    let script = {
        let script = directory.path().join("mock-avp.sh");
        std::fs::write(
            &script,
            b"#!/bin/sh\nfor arg in \"$@\"; do case \"$arg\" in /R:*) report=\"${arg#/R:}\";; esac; done\nprintf 'Detected: Trojan.Win32.Mock /sample.bin\\n' > \"$report\"\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).unwrap();
        script
    };

    let outcome = scan_paths(
        Some(&script),
        &[sample.as_path()],
        Duration::from_secs(5),
        None,
    );
    match outcome {
        ScanOutcome::Threats(threats) => assert!(threats[0].contains("Trojan")),
        other => panic!("unexpected scan outcome: {other:?}"),
    }
}
