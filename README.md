# ask-codex-sessions

Local CLI for searching prior Codex CLI sessions stored under `~/.codex`.

It is designed for questions like:

- "What was the latest spec for the orchestration interface?"
- "Find earlier discussions about making the interface simpler."
- "Show me the exact session and quote where we chose Rust."

The tool reads:

- thread metadata from `~/.codex/state_5.sqlite`
- rollout files from `~/.codex/sessions/`

It writes search results as JSON artifacts into:

- `./ask-codex-session-responses/`

By default, the command prints only the path to the JSON artifact on `stdout`.

## What It Does

The CLI searches old Codex sessions and returns:

- the Codex session id
- a `codex resume <session_id>` command
- the absolute path to the rollout file
- the chunk id that matched
- source line numbers inside the rollout file
- a quoted snippet
- ranking metadata and scores
- optional summaries
- an optional direct answer to the original query

## Search Modes

The public commands are named after the retrieval strategy they use.

| Command | Meaning | When to use |
| --- | --- | --- |
| `bm25llm` | Gemini query planning + SQLite FTS/BM25 retrieval + Gemini reranking | Best general default |
| `bm25llm-recent` | Same as `bm25llm`, but with a stronger recency bias | Best for "latest spec" or "most recent discussion" |
| `bm25` | Pure local BM25/FTS retrieval, no Gemini calls | Fastest, cheapest baseline |
| `llm` | Gemini reviews filtered chunks directly | Most expensive, most semantic |

The current default Gemini model is:

- `gemini-3-flash-preview`

## Requirements

- Rust toolchain
- local Codex history under `~/.codex`
- Gemini CLI installed and authenticated if you want to use `bm25llm`, `bm25llm-recent`, `llm`, `--sum`, or `-a`

The current code defaults to:

- state DB: `/home/kirill/.codex/state_5.sqlite`
- sessions root: `/home/kirill/.codex/sessions`

## Install

If you do not already have Rust and Cargo:

1. Install Rust with `rustup`:

```bash
curl https://sh.rustup.rs -sSf | sh
```

2. Restart your shell, or load Cargo into the current shell:

```bash
source "$HOME/.cargo/env"
```

3. Verify the tools are available:

```bash
rustc --version
cargo --version
```

4. Build and install this project from the repository root:

```bash
cargo install --path .
```

That installs the binary into:

- `~/.cargo/bin/ask-codex-sessions`

If `~/.cargo/bin` is not already on your `PATH`, add this line to your shell config:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

Then restart your shell and verify the binary:

```bash
ask-codex-sessions help
```

If you already have Rust and only want to build locally without installing:

```bash
cargo build
```

Then run it with:

```bash
cargo run -- help
```

## Build

```bash
cargo build
```

You can also run everything directly with `cargo run -- ...`.

## Help

Root help:

```bash
cargo run -- help
```

Per-command help:

```bash
cargo run -- bm25llm --help
cargo run -- bm25llm-recent --help
cargo run -- bm25 --help
cargo run -- llm --help
```

## Quick Start

General hybrid search:

```bash
cargo run -- bm25llm "firebase orchestration interface"
```

Latest-spec search:

```bash
cargo run -- bm25llm-recent "what was the latest spec for the interface"
```

Pure BM25 search:

```bash
cargo run -- bm25 "rust sqlite gemini"
```

LLM-only chunk review:

```bash
cargo run -- llm "find discussions about simplifying the interface"
```

## Common Flags

All search commands support:

- `--cwd <PATH>`
  - restrict search to sessions whose recorded `cwd` exactly matches this path
- `--since-days <DAYS>`
  - only search sessions newer than the given number of days
- `--limit <N>`
  - cap the number of ranked results
- `--debug`
  - print the pipeline stages and ranking details to `stderr`
- `--sum`
  - add summaries to the JSON output
- `-a`, `--answer`
  - add a direct answer to the original query in the JSON output

Examples:

```bash
cargo run -- bm25llm --cwd /home/kirill/p/ask-codex-sessions --since-days 90 --limit 3 "firebase orchestration interface"
```

```bash
cargo run -- bm25llm-recent --sum -a --cwd /home/kirill/p/ask-codex-sessions --since-days 90 "what tech stack did we choose for the session search tool"
```

```bash
cargo run -- bm25 --debug --cwd /home/kirill/p/ask-codex-sessions --since-days 90 "rust sqlite gemini"
```

## Output

Every non-debug run writes a JSON artifact to:

- `./ask-codex-session-responses/<timestamp>-<preset>-<mode>-<slug>.json`

The command prints the file path, for example:

```bash
/home/kirill/p/ask-codex-sessions/ask-codex-session-responses/20260307T023647Z-Search-Lexical-what-tech-stack-did-we-choose-for-the-session-search-tool.json
```

The JSON contains:

- top-level query metadata
- mode and preset
- optional top-level `summary`
- optional top-level `answer`
- ranked `results`

Each result contains:

- `rank`
- `session_id`
- `thread_id`
- `resume_command`
- `session_path`
- `text_id`
- `source_start_line`
- `source_end_line`
- `title`
- `created_at`
- `created_at_iso`
- `quote`
- optional `summary`
- `score`
- `metadata`

Typical `jq` usage:

Show the top hit:

```bash
jq '.results[0]' ask-codex-session-responses/<file>.json
```

Show the main fields you usually care about:

```bash
jq '.results[] | {rank, session_id, resume_command, session_path, text_id, quote}' ask-codex-session-responses/<file>.json
```

Show only summaries:

```bash
jq '{summary, answer, results: [.results[] | {rank, text_id, summary}]}' ask-codex-session-responses/<file>.json
```

Extract the rollout path for direct inspection:

```bash
jq -r '.results[0].session_path' ask-codex-session-responses/<file>.json
```

Resume the top session in Codex:

```bash
codex resume "$(jq -r '.results[0].session_id' ask-codex-session-responses/<file>.json)"
```

## How It Works

At a high level:

1. load thread metadata from `~/.codex/state_5.sqlite`
2. filter by `cwd` and time range
3. parse rollout JSONL files into user/assistant chunks
4. extract code-like entities such as paths, commands, and identifiers
5. build an in-memory SQLite FTS index
6. run one of the configured retrieval modes
7. generate a JSON artifact with citations

Mode details:

- `bm25llm`
  - Gemini creates a constrained query plan from the filtered corpus
  - SQLite FTS/BM25 retrieves candidates
  - Gemini reranks those candidates
- `bm25llm-recent`
  - same as `bm25llm`
  - ranking gives a stronger recency bonus
- `bm25`
  - uses local query expansion and SQLite FTS/BM25 only
- `llm`
  - Gemini judges filtered chunks directly instead of relying on SQLite ranking first

## Current Behavior and Limitations

- Filtering by repository is currently implemented as exact `cwd` equality, not by Git remote or fuzzy repo matching.
- The search index is rebuilt in memory on each run. There is no persistent incremental index yet.
- `bm25` is useful as a baseline, but it is less reliable than `bm25llm` or `llm` for questions where the answer chunk does not reuse the same wording as the query.
- Hybrid and LLM modes depend on the local `gemini` CLI.
- The tool is local-first and read-only with respect to Codex history.

## Development

Run the full test suite:

```bash
cargo test -- --nocapture
```

Useful targeted tests:

```bash
cargo test test_cli_has_search_and_latest_spec --test cli_contract -- --exact --nocapture
cargo test test_hybrid_search_pipeline_finds_current_session --test search_pipeline -- --exact --nocapture
cargo test test_lexical_mode_finds_current_session_without_gemini_planner --test lexical_mode -- --exact --nocapture
cargo test test_llm_search_mode_finds_current_session_by_chunk_judging --test llm_search_mode -- --exact --nocapture
```

## Repository Layout

- `src/cli.rs`
  - CLI definitions and help text
- `src/main.rs`
  - command dispatch and artifact writing
- `src/source.rs`
  - loads thread metadata from Codex SQLite state
- `src/normalize.rs`
  - parses rollout JSONL files into chunks and entities
- `src/index.rs`
  - in-memory SQLite FTS/BM25 index and ranking
- `src/search.rs`
  - retrieval pipeline
- `src/gemini.rs`
  - Gemini CLI integration
- `src/output.rs`
  - JSON artifact schema and writing
- `tests/`
  - fixture-driven regression tests

## Recommended Starting Point

If you just want the best default:

```bash
cargo run -- bm25llm --since-days 90 "your question here"
```

If you specifically care about the newest decision:

```bash
cargo run -- bm25llm-recent --since-days 90 "your question here"
```
