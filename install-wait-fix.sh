#!/usr/bin/env bash

# Installs the event-driven wait_agent fix. Pass --uninstall to restore vanilla Codex.
set -euo pipefail

CODEX_FIX_REPO="https://github.com/AlexanderDzhoganov/codex.git"
CODEX_FIX_REF="${CODEX_FIX_REF:-f41aa9bb20a3d5c94e1233e7a2b43ab80e8d8eb9}"
CODEX_FIX_BIN_DIR="$HOME/.local/bin"
CODEX_FIX_STATE_DIR="$HOME/.local/share/codex-wait-fix"
CODEX_FIX_STATE_FILE="$CODEX_FIX_STATE_DIR/installed-ref"
VANILLA_INSTALLER_URL="https://chatgpt.com/codex/install.sh"

step() {
  printf '==> %s\n' "$1"
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf '%s is required to install Codex.\n' "$1" >&2
    exit 1
  fi
}

pick_profile() {
  case "$(uname -s):${SHELL:-}" in
    Darwin:*/zsh) printf '%s\n' "$HOME/.zprofile" ;;
    Darwin:*/bash) printf '%s\n' "$HOME/.bash_profile" ;;
    Linux:*/zsh) printf '%s\n' "$HOME/.zshrc" ;;
    Linux:*/bash) printf '%s\n' "$HOME/.bashrc" ;;
    *) printf '%s\n' "$HOME/.profile" ;;
  esac
}

add_to_path() {
  local profile path_line
  profile="$(pick_profile)"
  path_line="export PATH=\"$CODEX_FIX_BIN_DIR:\$PATH\""
  if ! grep -Fqx "$path_line" "$profile" 2>/dev/null; then
    {
      printf '\n# >>> Codex installer >>>\n'
      printf '%s\n' "$path_line"
      printf '# <<< Codex installer <<<\n'
    } >>"$profile"
  fi
  printf 'PATH updated in %s; open a new terminal to use it.\n' "$profile"
}

install_vanilla() {
  step "Removing the custom Codex build"
  if [ -f "$CODEX_FIX_STATE_FILE" ]; then
    rm -f -- "$CODEX_FIX_BIN_DIR/codex" "$CODEX_FIX_BIN_DIR/codex-code-mode-host"
    rm -f -- "$CODEX_FIX_STATE_FILE"
    rmdir "$CODEX_FIX_STATE_DIR" 2>/dev/null || true
  fi

  step "Installing vanilla Codex from OpenAI"
  curl -fsSL "$VANILLA_INSTALLER_URL" | CODEX_NON_INTERACTIVE=true sh
}

case "${1:-}" in
  --uninstall)
    require_command curl
    install_vanilla
    exit 0
    ;;
  "") ;;
  *)
    printf 'Usage: install-wait-fix.sh [--uninstall]\n' >&2
    exit 2
    ;;
esac

require_command git
require_command curl

if ! command -v cargo >/dev/null 2>&1; then
  step "Installing Rust"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck source=/dev/null
  . "$HOME/.cargo/env"
fi

build_dir="$(mktemp -d "${TMPDIR:-/tmp}/codex-wait-fix.XXXXXX")"
cleanup() {
  rm -rf -- "$build_dir"
}
trap cleanup EXIT INT TERM

step "Downloading the fixed Codex source"
git -C "$build_dir" init -q
git -C "$build_dir" remote add origin "$CODEX_FIX_REPO"
git -C "$build_dir" fetch -q --depth 1 origin "$CODEX_FIX_REF"
git -C "$build_dir" checkout -q --detach FETCH_HEAD

step "Building Codex (this can take several minutes)"
(
  cd "$build_dir/codex-rs"
  cargo build --locked --release \
    -p codex-cli --bin codex \
    -p codex-code-mode-host --bin codex-code-mode-host
)

step "Installing Codex to $CODEX_FIX_BIN_DIR"
mkdir -p "$CODEX_FIX_BIN_DIR"
install -m 0755 "$build_dir/codex-rs/target/release/codex" "$CODEX_FIX_BIN_DIR/codex"
install -m 0755 \
  "$build_dir/codex-rs/target/release/codex-code-mode-host" \
  "$CODEX_FIX_BIN_DIR/codex-code-mode-host"
mkdir -p "$CODEX_FIX_STATE_DIR"
printf '%s\n' "$CODEX_FIX_REF" >"$CODEX_FIX_STATE_FILE"
add_to_path

step "Installed $($CODEX_FIX_BIN_DIR/codex --version)"
