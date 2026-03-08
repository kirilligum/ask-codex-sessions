// TEST-001
use ask_codex_sessions::cli::{try_parse_cli_from, Cli, Command};
use clap::CommandFactory;

#[test]
fn test_cli_has_search_and_latest_spec() {
    let command = Cli::command();
    let subcommands = command.get_subcommands().map(|sub| sub.get_name()).collect::<Vec<_>>();
    assert_eq!(subcommands, vec!["bm25llm", "bm25llm-recent", "bm25", "llm"]);

    let mut command = Cli::command();
    let help = command.render_long_help().to_string();
    assert!(!help.contains("Modes:"));
    assert!(help.contains("Hybrid search: Gemini query plan + SQLite FTS/BM25 + Gemini rerank"));
    assert!(help.contains("Pure local BM25/FTS search, no Gemini calls"));
    assert!(help.contains("LLM-only chunk review: Gemini judges filtered chunks directly"));
    assert!(help.contains("Examples:"));
    assert!(help.contains("Defaults to bm25llm with --since-days 30 and --answer"));

    let parsed = try_parse_cli_from([
        "ask-codex-sessions",
        "bm25llm-recent",
        "-d",
        "-s",
        "-a",
        "-C",
        "/tmp/project",
        "-t",
        "30",
        "-l",
        "2",
        "-o",
        "/tmp/out",
        "latest interface spec",
    ])
    .expect("bm25llm-recent should parse");
    match parsed.command {
        Command::Bm25llmRecent(args) => {
            assert_eq!(args.query, "latest interface spec");
            assert!(args.debug);
            assert!(args.sum);
            assert!(args.answer);
            assert_eq!(args.cwd.as_deref(), Some(std::path::Path::new("/tmp/project")));
            assert_eq!(args.since_days, Some(30));
            assert_eq!(args.limit, 2);
            assert_eq!(args.out_dir.as_deref(), Some(std::path::Path::new("/tmp/out")));
        }
        other => panic!("expected bm25llm-recent command, got {other:?}"),
    }

    let defaulted = try_parse_cli_from([
        "ask-codex-sessions",
        "-C",
        "/tmp/project",
        "latest interface spec",
    ])
    .expect("default mode should parse");
    match defaulted.command {
        Command::Bm25llm(args) => {
            assert_eq!(args.query, "latest interface spec");
            assert!(args.answer);
            assert_eq!(args.since_days, Some(30));
            assert_eq!(args.cwd.as_deref(), Some(std::path::Path::new("/tmp/project")));
            assert!(args.out_dir.is_none());
        }
        other => panic!("expected default bm25llm command, got {other:?}"),
    }
}
