// SPDX-License-Identifier: LGPL-2.1-or-later
//! The committed 7z fixture (written once by sevenz-rust's writer) opens
//! through the public archive API.

use lautta_core::entry::Kind;
use lautta_core::provider::archive::{ArchiveFormat, ArchiveProvider};
use lautta_core::provider::memory::MemoryProvider;
use lautta_core::provider::{read_all, Lane, Provider};
use lautta_core::VPath;
use std::sync::Arc;

const FIXTURE: &[u8] = include_bytes!("fixtures/sample.7z");

#[tokio::test]
async fn sevenz_fixture_lists_and_reads() {
    let tmp = tempfile::tempdir().unwrap();
    let mem = MemoryProvider::default();
    mem.add_file("sample.7z", FIXTURE, 0);
    let path = VPath::parse(b"sample.7z").unwrap();
    let p = ArchiveProvider::open(Arc::new(mem), path, tmp.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(p.format(), ArchiveFormat::SevenZ);
    let entries = p.entries();
    let listed: Vec<(String, Kind)> = entries.iter().map(|e| (e.path.display(), e.kind)).collect();
    assert_eq!(
        listed,
        [
            ("docs".to_owned(), Kind::Dir),
            ("docs/readme.md".to_owned(), Kind::File),
            ("top.txt".to_owned(), Kind::File)
        ]
    );
    let h = p
        .open_read(&VPath::parse(b"docs/readme.md").unwrap(), Lane::Bulk)
        .await
        .unwrap();
    assert_eq!(read_all(h.as_ref(), 1 << 20).await.unwrap(), b"# hello from 7z\n");
}
