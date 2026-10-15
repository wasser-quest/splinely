#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export TMPDIR="${TMPDIR:-$PWD/.work-tmp}"
mkdir -p "$TMPDIR"
cargo test --workspace
node --test runtime/tests/*.test.cjs crates/splinely_cli/tests/*.test.cjs
cargo build -p splinely_runtime
case "$(uname -s)" in
  Darwin) native_lib=target/debug/libsplinely_runtime.dylib; native_bin=target/debug/splinely-runtime ;;
  MINGW*|MSYS*|CYGWIN*) native_lib=target/debug/splinely_runtime.dll; native_bin=target/debug/splinely-runtime.exe ;;
  *) native_lib=target/debug/libsplinely_runtime.so; native_bin=target/debug/splinely-runtime ;;
esac
python3 examples/native-runtime/smoke.py --library "$native_lib" --binary "$native_bin"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
# Export through the CLI into temporary files, leaving checked-in examples alone.
bash scripts/generate-demo.sh "$work"
cargo run --quiet -p splinely-cli -- inspect "$work/demo.splinely.json"
cargo run --quiet -p splinely-cli -- evaluate "$work/demo.splinely.json" --time 1.5
python3 scripts/check-export.py "$work/demo.splinely.json" "$work/demo.svg"
cargo run --quiet -p splinely-cli -- validate examples/demo.splinely.json
python3 scripts/check-export.py examples/demo.splinely.json examples/demo.svg
printf '%s\n' 'PASS: automated checks (browser interaction/audio still require a real browser test).'
