#!/usr/bin/env bash
set -euo pipefail

REPO_URL="${ASK_CODEX_SESSIONS_REPO_URL:-https://github.com/kirilligum/ask-codex-sessions.git}"
BIN_DIR="${HOME}/.cargo/bin"
BIN_NAME="ask-codex-sessions"

if ! command -v cargo >/dev/null 2>&1; then
  cat >&2 <<'EOF'
cargo was not found.

Install Rust and Cargo first:
  curl https://sh.rustup.rs -sSf | sh

Then load Cargo into your shell:
  bash/zsh: source "$HOME/.cargo/env"
  fish:     source "$HOME/.cargo/env.fish"
EOF
  exit 1
fi

echo "Installing ${BIN_NAME} from ${REPO_URL}..."
cargo install --git "${REPO_URL}" --locked

cat <<EOF

Installed:
  ${BIN_DIR}/${BIN_NAME}

If ${BIN_DIR} is not on your PATH:
  bash/zsh: export PATH="\$HOME/.cargo/bin:\$PATH"
  fish:     fish_add_path "\$HOME/.cargo/bin"

Verify:
  ${BIN_NAME} help
EOF
