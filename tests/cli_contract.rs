// TEST-001
use ask_codex_sessions::cli::{Cli, Command};
use clap::{CommandFactory, Parser};

#[test]
fn test_cli_has_search_and_latest_spec() {
    let command = Cli::command();
    let subcommands = command.get_subcommands().map(|sub| sub.get_name()).collect::<Vec<_>>();
    assert_eq!(subcommands, vec!["search", "latest-spec"]);

    let parsed = Cli::try_parse_from([
        "ask-codex-sessions",
        "latest-spec",
        "--cwd",
        "/tmp/project",
        "--since-days",
        "30",
        "--limit",
        "2",
        "latest interface spec",
    ])
    .expect("latest-spec should parse");
    match parsed.command {
        Command::LatestSpec(args) => {
            assert_eq!(args.query, "latest interface spec");
            assert_eq!(args.cwd.as_deref(), Some(std::path::Path::new("/tmp/project")));
            assert_eq!(args.since_days, Some(30));
            assert_eq!(args.limit, 2);
        }
        other => panic!("expected latest-spec command, got {other:?}"),
    }
}
