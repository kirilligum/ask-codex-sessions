// TEST-002
use ask_codex_sessions::config::Config;
use std::path::PathBuf;

#[test]
fn test_config_uses_local_codex_defaults() {
    let config = Config::default();
    assert_eq!(config.state_db_path, PathBuf::from("/home/kirill/.codex/state_5.sqlite"));
    assert_eq!(config.sessions_root, PathBuf::from("/home/kirill/.codex/sessions"));
    assert_eq!(config.gemini_model, "gemini-3-flash-preview");
}
