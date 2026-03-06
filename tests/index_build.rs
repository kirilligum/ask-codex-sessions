// TEST-005
use ask_codex_sessions::index::SearchIndex;
use ask_codex_sessions::normalize::normalize_thread;
use ask_codex_sessions::source::load_threads;
use std::collections::HashSet;
use std::path::Path;

#[test]
fn test_build_small_sqlite_index_with_fts() {
    let threads = load_threads(Path::new("tests/fixtures/state_5.sqlite")).expect("fixture threads should load");
    let mut chunks = Vec::new();
    for thread in &threads {
        chunks.extend(normalize_thread(thread).expect("fixtures should normalize"));
    }
    let index = SearchIndex::build(&threads, &chunks).expect("index should build");
    let names = index.table_names().expect("table names should load").into_iter().collect::<HashSet<_>>();
    assert!(names.contains("sessions"));
    assert!(names.contains("chunks"));
    assert!(names.contains("fts_chunks"));
    assert!(!chunks.is_empty());
}
