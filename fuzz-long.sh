#!/usr/bin/env bash
# Long-running fuzz session for the socket_http target, intended to be run
# manually by maintainers before a release. CI only fuzzes for ~10 minutes.
#
# Usage: ./fuzz-long.sh [SECONDS]
#
#   SECONDS       Total fuzzing time. Default: 14400 (4 hours).
#
# Environment:
#   WORKERS       Parallel fuzzing processes. Default: half the CPU cores.
#   CORPUS_DIR    Corpus location, kept between runs. Default: fuzz/corpus/socket_http.
#
# Requires a nightly Rust toolchain and cargo-fuzz:
#   rustup toolchain install nightly && cargo install cargo-fuzz --locked
#
# Exits non-zero if a crash, timeout or OOM is found. Reproduce with:
#   cargo +nightly fuzz run socket_http fuzz/artifacts/socket_http/<artifact>

set -euo pipefail

TARGET=socket_http
# Matches fuzz/socket_http.options: 10 MiB body limit plus 64 KiB of HTTP framing.
MAX_LEN=10551296

cd "$(dirname "${BASH_SOURCE[0]}")"

seconds="${1:-14400}"
if ! [[ "$seconds" =~ ^[1-9][0-9]*$ ]]; then
    echo "error: SECONDS must be a positive integer, got '$seconds'" >&2
    exit 2
fi

cores="$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 2)"
workers="${WORKERS:-$((cores / 2 > 0 ? cores / 2 : 1))}"
if ! [[ "$workers" =~ ^[1-9][0-9]*$ ]]; then
    echo "error: WORKERS must be a positive integer, got '$workers'" >&2
    exit 2
fi

corpus="${CORPUS_DIR:-fuzz/corpus/$TARGET}"
artifacts="fuzz/artifacts/$TARGET"

if ! rustup toolchain list 2>/dev/null | grep -q '^nightly'; then
    echo "error: a nightly Rust toolchain is required (rustup toolchain install nightly)" >&2
    exit 1
fi
if ! cargo +nightly fuzz --version >/dev/null 2>&1; then
    echo "error: cargo-fuzz is required (cargo install cargo-fuzz --locked)" >&2
    exit 1
fi

mkdir -p "$corpus" "$artifacts"
# Seed without overwriting inputs that earlier runs have already evolved.
cp -n fuzz/seeds/$TARGET/* "$corpus"/

start="$(date +%s)"
echo "Fuzzing $TARGET for ${seconds}s with $workers worker(s); corpus: $corpus"

status=0
# Each exchange waits up to 100 ms for incomplete requests, so a 60 s per-input
# timeout only trips on a genuine hang.
cargo +nightly fuzz run "$TARGET" "$corpus" -- \
    -fork="$workers" \
    -max_total_time="$seconds" \
    -max_len="$MAX_LEN" \
    -timeout=60 \
    -rss_limit_mb=2560 \
    -print_final_stats=1 || status=$?

# Crash artifacts are the authoritative result; fork mode can exit 0 after a
# worker crashed and was restarted.
new_artifacts="$(find "$artifacts" -type f -newermt "@$start" 2>/dev/null | sort)"
if [[ -n "$new_artifacts" ]]; then
    echo >&2
    echo "FAILURE: the fuzzer found problems in this run:" >&2
    echo "$new_artifacts" >&2
    echo "Reproduce with: cargo +nightly fuzz run $TARGET <artifact>" >&2
    exit 1
fi

if [[ "$status" -ne 0 ]]; then
    echo "FAILURE: cargo fuzz exited with status $status" >&2
    exit "$status"
fi

echo "OK: no crashes, timeouts or OOMs in $(($(date +%s) - start))s."
