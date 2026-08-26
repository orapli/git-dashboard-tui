#!/bin/sh
# git-dashboard-tui installer.
#
#   curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | sh
#
# Downloads the pre-built binary for your platform from GitHub Releases and
# installs it. No Rust toolchain required, and nothing is run with elevated
# privileges unless you point INSTALL_DIR at a directory that needs it.
#
# Environment variables:
#   VERSION      release tag to install            (default: latest)
#   INSTALL_DIR  where to put the binary           (default: ~/.local/bin,
#                                                   or /usr/local/bin if that
#                                                   is writable and ~/.local/bin
#                                                   is not on PATH)
#
# Piping a script to a shell means trusting it. Read it first if you like:
#   curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | less

set -eu

REPO="orapli/git-dashboard-tui"
BIN="git-dashboard-tui"
VERSION="${VERSION:-latest}"

# ---------------------------------------------------------------- utilities

RED=''
YELLOW=''
GREEN=''
BOLD=''
RESET=''
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    RED=$(printf '\033[31m')
    YELLOW=$(printf '\033[33m')
    GREEN=$(printf '\033[32m')
    BOLD=$(printf '\033[1m')
    RESET=$(printf '\033[0m')
fi

info() { printf '%s\n' "$*"; }
warn() { printf '%swarning:%s %s\n' "$YELLOW" "$RESET" "$*" >&2; }
die() {
    printf '%serror:%s %s\n' "$RED" "$RESET" "$*" >&2
    exit 1
}

# Everything lands in a temp dir that is removed on any exit path.
TMPDIR_INSTALL=''
cleanup() { [ -n "$TMPDIR_INSTALL" ] && rm -rf "$TMPDIR_INSTALL"; }
trap cleanup EXIT INT TERM

fetch() {
    # fetch <url> <output-path>
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$1" -o "$2"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$2" "$1"
    else
        die "neither curl nor wget is available"
    fi
}

# ------------------------------------------------------------ platform detect

os=$(uname -s)
arch=$(uname -m)

case "$os" in
    Linux) os_part='unknown-linux-gnu' ;;
    Darwin) os_part='apple-darwin' ;;
    MINGW* | MSYS* | CYGWIN*)
        die "Windows is not supported by this script.
  Download git-dashboard-tui-x86_64-pc-windows-msvc.zip from
  https://github.com/$REPO/releases/latest and extract $BIN.exe"
        ;;
    *) die "unsupported operating system: $os" ;;
esac

case "$arch" in
    x86_64 | amd64) arch_part='x86_64' ;;
    aarch64 | arm64) arch_part='aarch64' ;;
    *) die "unsupported architecture: $arch
  Pre-built binaries exist for x86_64 and aarch64 only.
  To build from source instead:
    cargo install --git https://github.com/$REPO --locked" ;;
esac

target="${arch_part}-${os_part}"
asset="${BIN}-${target}.tar.gz"

if [ "$VERSION" = "latest" ]; then
    base_url="https://github.com/$REPO/releases/latest/download"
else
    base_url="https://github.com/$REPO/releases/download/$VERSION"
fi

# ------------------------------------------------------------- install target

if [ -n "${INSTALL_DIR:-}" ]; then
    install_dir="$INSTALL_DIR"
else
    install_dir="$HOME/.local/bin"
    # Prefer /usr/local/bin only when it is already writable *and* the default
    # is not on PATH — never escalate to sudo on the user's behalf.
    case ":${PATH}:" in
        *":$HOME/.local/bin:"*) ;;
        *) [ -w /usr/local/bin ] && install_dir=/usr/local/bin ;;
    esac
fi

# ------------------------------------------------------------------- download

info "${BOLD}Installing $BIN${RESET} ($target, $VERSION)"

TMPDIR_INSTALL=$(mktemp -d 2>/dev/null || mktemp -d -t gdt)
archive="$TMPDIR_INSTALL/$asset"

info "  downloading $asset"
fetch "$base_url/$asset" "$archive" ||
    die "download failed: $base_url/$asset
  Check that a release exists at https://github.com/$REPO/releases"

# Verify against the release checksum file when one is published. Older
# releases predate it, so a missing file is a warning rather than a failure.
if fetch "$base_url/SHA256SUMS" "$TMPDIR_INSTALL/SHA256SUMS" 2>/dev/null; then
    if command -v sha256sum >/dev/null 2>&1; then
        sha_cmd='sha256sum'
    elif command -v shasum >/dev/null 2>&1; then
        sha_cmd='shasum -a 256'
    else
        sha_cmd=''
    fi
    if [ -n "$sha_cmd" ]; then
        expected=$(grep " \{1,2\}\*\{0,1\}$asset\$" "$TMPDIR_INSTALL/SHA256SUMS" | awk '{print $1}')
        if [ -n "$expected" ]; then
            actual=$(cd "$TMPDIR_INSTALL" && $sha_cmd "$asset" | awk '{print $1}')
            [ "$expected" = "$actual" ] ||
                die "checksum mismatch for $asset
  expected $expected
  actual   $actual
  Refusing to install. Please report this at https://github.com/$REPO/issues"
            info "  checksum verified"
        else
            warn "no checksum listed for $asset; continuing"
        fi
    else
        warn "no sha256sum/shasum available; skipping checksum verification"
    fi
else
    warn "this release publishes no SHA256SUMS; skipping checksum verification"
fi

info "  extracting"
tar -xzf "$archive" -C "$TMPDIR_INSTALL" || die "failed to extract $asset"
[ -f "$TMPDIR_INSTALL/$BIN" ] || die "archive did not contain $BIN"
chmod +x "$TMPDIR_INSTALL/$BIN"

# Confirm the binary actually runs on this machine before installing it, so a
# wrong-architecture download fails here instead of at first use.
if ! "$TMPDIR_INSTALL/$BIN" --version >/dev/null 2>&1; then
    # --version may be unsupported; fall back to checking it is executable at all.
    "$TMPDIR_INSTALL/$BIN" --help >/dev/null 2>&1 ||
        warn "the downloaded binary did not respond to --version/--help; installing anyway"
fi

# -------------------------------------------------------------------- install

mkdir -p "$install_dir" 2>/dev/null ||
    die "cannot create $install_dir
  Choose a different location, e.g.:
    curl -fsSL https://raw.githubusercontent.com/$REPO/main/install.sh | INSTALL_DIR=\$HOME/bin sh"

if [ ! -w "$install_dir" ]; then
    die "$install_dir is not writable.
  Either pick a writable location:
    curl -fsSL https://raw.githubusercontent.com/$REPO/main/install.sh | INSTALL_DIR=\$HOME/.local/bin sh
  or install there yourself:
    sudo install -m755 <extracted $BIN> $install_dir/$BIN"
fi

# install(1) replaces the file atomically, which matters when upgrading a
# copy that is currently running.
if command -v install >/dev/null 2>&1; then
    install -m 755 "$TMPDIR_INSTALL/$BIN" "$install_dir/$BIN"
else
    cp "$TMPDIR_INSTALL/$BIN" "$install_dir/$BIN"
    chmod 755 "$install_dir/$BIN"
fi

info "${GREEN}Installed${RESET} $install_dir/$BIN"

# ---------------------------------------------------------------- PATH advice

case ":${PATH}:" in
    *":$install_dir:"*)
        info ""
        info "Run ${BOLD}$BIN${RESET} to start."
        ;;
    *)
        info ""
        warn "$install_dir is not on your PATH."
        info "  Add it by appending this to your shell profile"
        info "  (~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish, ...):"
        info ""
        info "    export PATH=\"$install_dir:\$PATH\""
        info ""
        info "  Or run it directly: $install_dir/$BIN"
        ;;
esac
