// TEST-003
use ask_codex_sessions::source::load_threads;
use std::path::PathBuf;

#[test]
fn test_ingest_thread_metadata_from_fixture_sqlite() {
    let threads = load_threads(PathBuf::from("tests/fixtures/state_5.sqlite").as_path())
        .expect("fixture SQLite should load");
    assert_eq!(threads.len(), 2);
    let current = threads
        .iter()
        .find(|thread| thread.thread_id == "019cc49c-0918-7c11-9a8a-630c28b9b443")
        .expect("current session thread should exist");
    assert_eq!(current.cwd, PathBuf::from("/home/kirill/p/ask-codex-sessions"));
    assert!(current.title.contains("find a conversation"));
    assert!(current.rollout_path.ends_with("tests/fixtures/current-session.jsonl"));
}
