//! End-to-end: local HTTP feed → check → signed download → install → rollback.
#![allow(clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::{fs, thread};

use kw_update::{AssetKind, Install, Progress, StartupOutcome, platform_key};
use semver::Version;

const PUB: &str = include_str!("fixtures/test-key.pub");
const DATA: &[u8] = include_bytes!("fixtures/hello.txt");
const SIG: &str = include_str!("fixtures/hello.txt.minisig");
const SHA: &str = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

/// Serves `/latest.json` (built from the base URL) and `/app.bin`.
fn serve(make_manifest: impl FnOnce(&str) -> String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let manifest = make_manifest(&base);
    thread::spawn(move || {
        for stream in listener.incoming().take(4) {
            let mut s = stream.unwrap();
            let mut line = String::new();
            BufReader::new(&s).read_line(&mut line).unwrap();
            let body: Vec<u8> = if line.contains("/latest.json") { manifest.clone().into_bytes() } else { DATA.to_vec() };
            write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
            s.write_all(&body).unwrap();
        }
    });
    base
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("kw-e2e").join(name);
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[cfg(unix)]
#[test]
fn full_update_cycle() {
    let base = serve(|base| {
        serde_json::json!({
            "version": "9.0.0",
            "notes": "test",
            "platforms": { platform_key(): { "url": format!("{base}/app.bin"), "sha256": SHA, "signature": SIG, "kind": "binary" } }
        })
        .to_string()
    });

    let update = kw_update::check(&format!("{base}/latest.json"), &Version::new(0, 1, 0)).unwrap().unwrap();
    assert_eq!(update.version, Version::new(9, 0, 0));
    assert_eq!(update.asset.kind, AssetKind::Binary);

    let d = tmp("cycle");
    let key = kw_update::public_key_from(Some(PUB)).unwrap();
    let file = kw_update::download_with_key(&update.asset, &d.join("dl"), &Progress::default(), &key).unwrap();
    assert_eq!(fs::read(&file).unwrap(), DATA);

    let current = d.join("kubernet-wizard");
    fs::write(&current, b"old build").unwrap();
    let state = d.join("state");
    let target = kw_update::apply(&file, update.asset.kind, &Install::Binary(current.clone()), "9.0.0", &state).unwrap();
    assert_eq!(target, current);
    assert_eq!(fs::read(&current).unwrap(), DATA);

    // New version keeps failing to start → rollback restores old build.
    for _ in 0..3 {
        assert!(matches!(kw_update::on_startup(&state, "9.0.0"), StartupOutcome::JustUpdated { .. }));
    }
    assert!(matches!(kw_update::on_startup(&state, "9.0.0"), StartupOutcome::RolledBack { .. }));
    assert_eq!(fs::read(&current).unwrap(), b"old build");
}

#[test]
fn tampered_trusted_comment_rejected() {
    let base = serve(|_| String::new());
    let asset: kw_update::Asset = serde_json::from_value(serde_json::json!({
        "url": format!("{base}/app.bin"), "sha256": SHA, "signature": SIG.replace("kw test", "tampered"), "kind": "binary"
    }))
    .unwrap();
    let key = kw_update::public_key_from(Some(PUB)).unwrap();
    let r = kw_update::download_with_key(&asset, &tmp("tamper"), &Progress::default(), &key);
    assert!(matches!(r, Err(kw_update::UpdateError::Signature(_))), "{r:?}");
}
