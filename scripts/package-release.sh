#!/usr/bin/env bash
# Build release binaries and pack them as dist/splinely-vX.Y.Z-<arch>-linux.tar.gz
# (plus a .sha256 file). Usage: scripts/package-release.sh [--no-build]
set -euo pipefail
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
name="splinely-v${version}-$(uname -m)-linux"
[ "${1:-}" = "--no-build" ] || cargo build --release --locked --workspace
stage="dist/$name"
rm -rf "$stage" "dist/$name.tar.gz" "dist/$name.tar.gz.sha256"
mkdir -p "$stage/bin" "$stage/lib" "$stage/include"
for bin in splinely splinely-cli splinely-viewer splinely-runtime; do
  install -m 755 "target/release/$bin" "$stage/bin/"
done
install -m 755 target/release/libsplinely_runtime.so "$stage/lib/"
install -m 644 crates/splinely_runtime/include/splinely_runtime.h "$stage/include/"
cp -r LICENSE README.md LLM.md CLI.md RUNTIME_API.md CORE_API.md docs/INSTALL-BINARIES.md prompts examples "$stage/"
rm -rf "$stage/examples/native-runtime/__pycache__"
tar -C dist -czf "dist/$name.tar.gz" "$name"
(cd dist && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
echo "dist/$name.tar.gz"
