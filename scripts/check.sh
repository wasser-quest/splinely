#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export TMPDIR="${TMPDIR:-$PWD/.work-tmp}"
mkdir -p "$TMPDIR"
cargo test --workspace
node --test runtime/tests/*.test.cjs crates/forma_cli/tests/*.test.cjs
cargo build -p forma_runtime
case "$(uname -s)" in
  Darwin) native_lib=target/debug/libforma_runtime.dylib; native_bin=target/debug/forma-runtime ;;
  MINGW*|MSYS*|CYGWIN*) native_lib=target/debug/forma_runtime.dll; native_bin=target/debug/forma-runtime.exe ;;
  *) native_lib=target/debug/libforma_runtime.so; native_bin=target/debug/forma-runtime ;;
esac
python3 examples/native-runtime/smoke.py --library "$native_lib" --binary "$native_bin"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
# Export through the CLI into temporary files, leaving checked-in examples alone.
bash scripts/generate-demo.sh "$work"
cargo run --quiet -p forma-cli -- inspect "$work/demo.forma.json"
cargo run --quiet -p forma-cli -- evaluate "$work/demo.forma.json" --time 1.5
python3 scripts/check-export.py "$work/demo.forma.json" "$work/demo.svg"
cargo run --quiet -p forma-cli -- validate examples/demo.forma.json
python3 scripts/check-export.py examples/demo.forma.json examples/demo.svg
printf '%s\n' 'PASS: automated checks (browser interaction/audio still require a real browser test).'
