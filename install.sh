#!/bin/sh
# Install the Floe compiler by downloading a prebuilt binary from the latest
# (or a pinned) GitHub Release and dropping it into a bin directory on $PATH.
#
#   curl -fsSL https://raw.githubusercontent.com/floeorg/floe/main/install.sh | sh
#
# Environment overrides:
#   FLOE_VERSION   tag to install (default: latest). Use e.g. "v0.5.4".
#   INSTALL_DIR    where to write the binary (default: "$HOME/.local/bin").

set -eu

REPO="floeorg/floe"
VERSION="${FLOE_VERSION:-latest}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

msg()  { printf '==> %s\n' "$*"; }
die()  { printf 'error: %s\n' "$*" >&2; exit 1; }

case "$(uname -s)" in
    Darwin) os="apple-darwin" ;;
    Linux)  os="unknown-linux-gnu" ;;
    *)      die "unsupported OS: $(uname -s). Download a prebuilt binary from https://github.com/$REPO/releases or build from source." ;;
esac

case "$(uname -m)" in
    x86_64|amd64)   arch="x86_64" ;;
    aarch64|arm64)  arch="aarch64" ;;
    *)              die "unsupported architecture: $(uname -m)" ;;
esac

target="${arch}-${os}"

if [ "$VERSION" = "latest" ]; then
    msg "Resolving latest release tag"
    VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
        | sed -n 's/.*"tag_name" *: *"\([^"]*\)".*/\1/p' \
        | head -n 1)
    [ -n "$VERSION" ] || die "failed to resolve latest tag — GitHub API may be rate-limiting. Set FLOE_VERSION explicitly (e.g. FLOE_VERSION=v0.5.4)."
fi

url="https://github.com/$REPO/releases/download/$VERSION/floe-${target}.tar.gz"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

msg "Downloading $url"
curl -fsSL "$url" -o "$tmp/floe.tar.gz" \
    || die "download failed. Check https://github.com/$REPO/releases/tag/$VERSION for available assets."

tar -xzf "$tmp/floe.tar.gz" -C "$tmp"
[ -f "$tmp/floe" ] || die "archive did not contain a 'floe' binary"

mkdir -p "$INSTALL_DIR"
mv "$tmp/floe" "$INSTALL_DIR/floe"
chmod +x "$INSTALL_DIR/floe"

msg "Installed floe $VERSION to $INSTALL_DIR/floe"

case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        # shellcheck disable=SC2016 # `$PATH` is intentionally literal — we're
        # printing the line the user should paste into their shell rc.
        printf '\nNote: %s is not on your $PATH. Add this to your shell rc:\n\n    export PATH="%s:$PATH"\n\n' "$INSTALL_DIR" "$INSTALL_DIR"
        ;;
esac

"$INSTALL_DIR/floe" --version
