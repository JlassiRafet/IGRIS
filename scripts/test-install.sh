#!/bin/sh
# Tests for install.sh with local fixtures: no network, nothing outside a temporary folder.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
fail() { echo "FAIL: $1" >&2; exit 1; }

# 1. The asset name for each supported machine, and a refusal for others.
want() { # uname -s, uname -m, expected asset
  got=$(IGRIS_UNAME_S=$1 IGRIS_UNAME_M=$2 IGRIS_VERSION=v9.9.9 IGRIS_DRY_RUN=1 sh "$here/install.sh")
  [ "${got##*/}" = "$3" ] || fail "$1 $2: got ${got##*/}, wanted $3"
}
want Linux x86_64 igris-v9.9.9-x86_64-unknown-linux-gnu.tar.gz
want Linux aarch64 igris-v9.9.9-aarch64-unknown-linux-gnu.tar.gz
want Darwin arm64 igris-v9.9.9-aarch64-apple-darwin.tar.gz
want Darwin x86_64 igris-v9.9.9-x86_64-apple-darwin.tar.gz
if IGRIS_UNAME_S=Plan9 IGRIS_UNAME_M=mips IGRIS_DRY_RUN=1 sh "$here/install.sh" 2>"$tmp/err"; then fail "an unknown platform was accepted"; fi
grep -q "no build for Plan9" "$tmp/err" || fail "the refusal does not say why"
echo "ok  asset names and the refusal"

# 2. A fixture release for this machine: install works, the binary runs.
asset=$(IGRIS_VERSION=vtest IGRIS_DRY_RUN=1 sh "$here/install.sh")
asset=${asset##*/}
mkdir -p "$tmp/rel" "$tmp/pkg"
printf '#!/bin/sh\necho "igris 9.9.9"\n' > "$tmp/pkg/igris"
chmod 755 "$tmp/pkg/igris"
tar -czf "$tmp/rel/$asset" -C "$tmp/pkg" igris
if command -v sha256sum >/dev/null 2>&1; then sum=$(sha256sum "$tmp/rel/$asset" | awk '{ print $1 }'); else sum=$(shasum -a 256 "$tmp/rel/$asset" | awk '{ print $1 }'); fi
printf '%s  %s\n' "$sum" "$asset" > "$tmp/rel/SHA256SUMS"
IGRIS_BASE_URL="file://$tmp/rel" IGRIS_VERSION=vtest IGRIS_INSTALL_DIR="$tmp/bin" sh "$here/install.sh" > "$tmp/out" || fail "the install failed: $(cat "$tmp/out")"
[ "$("$tmp/bin/igris" --version)" = "igris 9.9.9" ] || fail "the installed binary does not run"
grep -q "add it to your PATH" "$tmp/out" || fail "no PATH hint when the folder is not on PATH"
echo "ok  install from a fixture release, binary runs, PATH hint"

# 3. A wrong checksum is refused and nothing is installed.
printf '%s  %s\n' "0000000000000000000000000000000000000000000000000000000000000000" "$asset" > "$tmp/rel/SHA256SUMS"
rm -rf "$tmp/bin2"
if IGRIS_BASE_URL="file://$tmp/rel" IGRIS_VERSION=vtest IGRIS_INSTALL_DIR="$tmp/bin2" sh "$here/install.sh" 2>"$tmp/err"; then fail "a wrong checksum was accepted"; fi
grep -q "does not match" "$tmp/err" || fail "the mismatch message is missing"
[ ! -e "$tmp/bin2/igris" ] || fail "a binary was installed despite the mismatch"
# And an asset missing from SHA256SUMS.
printf '%s  %s\n' "$sum" "some-other-file.tar.gz" > "$tmp/rel/SHA256SUMS"
if IGRIS_BASE_URL="file://$tmp/rel" IGRIS_VERSION=vtest IGRIS_INSTALL_DIR="$tmp/bin2" sh "$here/install.sh" 2>"$tmp/err"; then fail "an unlisted asset was accepted"; fi
echo "ok  a wrong or missing checksum installs nothing"
echo "test-install: all checks passed"
