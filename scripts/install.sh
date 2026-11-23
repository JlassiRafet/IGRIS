#!/bin/sh
# Install igris on Linux or macOS: download the release archive for this machine, check its
# SHA-256 against the release's SHA256SUMS, and put `igris` in a folder of your own.
#
#   curl -fsSL https://raw.githubusercontent.com/JlassiRafet/IGRIS/main/scripts/install.sh | sh
#
# Environment (all optional):
#   IGRIS_VERSION      a tag such as v0.1.0 (default: the latest release; a pre-release needs this)
#   IGRIS_INSTALL_DIR  where to put the binary (default: ~/.local/bin)
#   IGRIS_DRY_RUN=1    print the download address and stop
#   IGRIS_REPO, IGRIS_BASE_URL, IGRIS_UNAME_S, IGRIS_UNAME_M  for tests
set -eu

REPO=${IGRIS_REPO:-JlassiRafet/IGRIS}
DIR=${IGRIS_INSTALL_DIR:-$HOME/.local/bin}
os=${IGRIS_UNAME_S:-$(uname -s)}
arch=${IGRIS_UNAME_M:-$(uname -m)}

case "$os-$arch" in
  Linux-x86_64 | Linux-amd64) target=x86_64-unknown-linux-gnu ;;
  Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-gnu ;;
  Darwin-arm64) target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  *)
    echo "igris: there is no build for $os on $arch (Linux and macOS, x86_64 or arm64)." >&2
    echo "       On Windows use install.ps1. Or build it: cargo install --git https://github.com/$REPO igris" >&2
    exit 1
    ;;
esac

if [ -n "${IGRIS_VERSION:-}" ]; then
  tag=$IGRIS_VERSION
else
  tag=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest" | sed 's#.*/tag/##')
  case "$tag" in
    v*) ;;
    *)
      echo "igris: no stable release found. For a pre-release: IGRIS_VERSION=v0.1.0-rc1 sh install.sh" >&2
      exit 1
      ;;
  esac
fi

asset="igris-$tag-$target.tar.gz"
base=${IGRIS_BASE_URL:-https://github.com/$REPO/releases/download/$tag}

if [ "${IGRIS_DRY_RUN:-}" = 1 ]; then
  echo "$base/$asset"
  exit 0
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "igris: downloading $asset"
curl -fsSL "$base/$asset" -o "$tmp/$asset"
curl -fsSL "$base/SHA256SUMS" -o "$tmp/SHA256SUMS"

expected=$(awk -v f="$asset" '$2 == f || $2 == "*" f { print $1 }' "$tmp/SHA256SUMS")
if [ -z "$expected" ]; then
  echo "igris: $asset is not listed in SHA256SUMS; not installing." >&2
  exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$tmp/$asset" | awk '{ print $1 }')
else
  actual=$(shasum -a 256 "$tmp/$asset" | awk '{ print $1 }')
fi
if [ "$expected" != "$actual" ]; then
  echo "igris: the checksum of $asset does not match SHA256SUMS; not installing." >&2
  exit 1
fi

tar -xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$DIR"
cp "$tmp/igris" "$DIR/igris"
chmod 755 "$DIR/igris"
echo "igris: installed $("$DIR/igris" --version) to $DIR/igris"

case ":$PATH:" in
  *":$DIR:"*) ;;
  *) echo "igris: add it to your PATH:  export PATH=\"$DIR:\$PATH\"   (put that line in your shell profile)" ;;
esac
echo "igris: run it with:  igris"
