#!/bin/bash
# Helpers for the Homebrew tests in install.yml and upgrade.yml. A local tap stands in for
# JonathansDev/homebrew-tap, and a folder of checksums stands in for GitHub Releases: test
# builds fetch from it because PHYLODE_TEST_CHECKSUM_URL_PREFIX points there (src/build.rs).
#
#   homebrew-test.sh build <version> [exit-after]  build this commit as <version>; with
#                                                  exit-after, a version that exits on purpose
#   homebrew-test.sh tap                           create the local tap
#   homebrew-test.sh offer <version> [tamper]      put <version> in the tap and publish its
#                                                  checksum; tamper changes one byte of the
#                                                  program in the archive after the checksum
#   homebrew-test.sh lines                         how many records the upgrade history has
#   homebrew-test.sh wait <lines>                  wait until the history has more than <lines>
#
# PHYLODE_TEST_WORK names a folder for all of it, without spaces: it ends up in a file:// URL.

set -euo pipefail

work="${PHYLODE_TEST_WORK:?set PHYLODE_TEST_WORK}"
checksums="$work/checksums"
history=/usr/local/libexec/phylode/history.tsv
repo="$(cd "$(dirname "$0")/../.." && pwd)"

tap_dir() { echo "$(brew --repository)/Library/Taps/jonathansdev/homebrew-tap"; }

case "${1:-}" in
  build)
    version="$2"
    exit_after="${3:-}"
    mkdir -p "$work/bin"
    cd "$repo"
    cp Cargo.toml "$work/Cargo.toml.orig"
    awk -v v="$version" '!done && /^version = /{print "version = \"" v "\""; done=1; next} {print}' \
      "$work/Cargo.toml.orig" > Cargo.toml
    if [ -n "$exit_after" ]; then export PHYLODE_TEST_EXIT_AFTER="$exit_after"; fi
    PHYLODE_TEST_CHECKSUM_URL_PREFIX="file://$checksums" cargo build --release
    cp target/release/phylode "$work/bin/$version"
    cp "$work/Cargo.toml.orig" Cargo.toml
    git checkout -q -- Cargo.lock
    "$work/bin/$version" --version
    ;;
  tap)
    brew tap-new --no-git jonathansdev/tap >/dev/null
    mkdir -p "$(tap_dir)/Formula"
    ;;
  offer)
    version="$2"
    dir="$work/offer/$version"
    mkdir -p "$dir" "$checksums/$version"
    cp "$work/bin/$version" "$dir/phylode"
    ( cd "$dir" && shasum -a 256 phylode > "$checksums/$version/phylode-$version-arm64.bin.sha256" )
    if [ "${3:-}" = tamper ]; then
      offset=4096
      byte=$(xxd -s "$offset" -l 1 -p "$dir/phylode")
      printf "$(printf '\\x%02x' $(( 0x$byte ^ 0xff )))" \
        | dd of="$dir/phylode" bs=1 seek="$offset" conv=notrunc 2>/dev/null
      if ( cd "$dir" && shasum -a 256 -c -s "$checksums/$version/phylode-$version-arm64.bin.sha256" ); then
        echo "::error::the tampered program still matches its checksum"; exit 1
      fi
    fi
    archive="$dir/phylode-$version-arm64.tar.gz"
    tar -C "$dir" -czf "$archive" phylode
    sha=$(shasum -a 256 "$archive" | cut -d' ' -f1)
    sed -e "s|@URL@|file://$archive|" -e "s|@VERSION@|$version|" -e "s|@SHA256@|$sha|" \
      "$repo/.github/homebrew/phylode.rb" > "$(tap_dir)/Formula/phylode.rb"
    ;;
  lines)
    if [ -f "$history" ]; then wc -l < "$history" | tr -d ' '; else echo 0; fi
    ;;
  wait)
    before="$2"
    start=$(date +%s)
    for _ in $(seq 1 240); do
      now=$(bash "$0" lines)
      if [ "$now" -gt "$before" ]; then
        echo "the helper recorded it after $(( $(date +%s) - start ))s:"
        tail -n $(( now - before )) "$history"
        exit 0
      fi
      sleep 1
    done
    echo "::error::the upgrade helper recorded nothing within 240 seconds"
    sudo tail -40 /var/log/phylode/upgrade.log || true
    exit 1
    ;;
  *)
    sed -n '2,15p' "$0"
    exit 2
    ;;
esac
