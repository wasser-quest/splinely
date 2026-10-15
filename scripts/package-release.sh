#!/usr/bin/env bash
# Build release binaries and pack them as dist/splinely-vX.Y.Z-<arch>-<os>.tar.gz
# (plus a .sha256 file), <os> = linux | macos. Prints the archive path.
#
# Usage: scripts/package-release.sh [--no-build]
#   SPLINELY_TARGET=<rust triple>   cross-build for another triple (e.g. x86_64-apple-darwin
#                                   on an Apple-silicon Mac); default: the host triple.
#   SPLINELY_UNIVERSAL=1            macOS only: build aarch64 + x86_64 and merge them with
#                                   `lipo` into dist/splinely-vX.Y.Z-universal-macos.tar.gz.
# Windows has its own script: scripts/package-release.ps1 (zip).
# macOS binaries are ad-hoc signed ("codesign -s -"), NOT notarized: see docs/INSTALL-BINARIES.md.
# The Linux path is unchanged; the macOS path has not been run on real macOS hardware yet.
set -euo pipefail
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
os=$(uname -s)
case "$os" in
  Linux) os_name=linux; lib=libsplinely_runtime.so ;;
  Darwin) os_name=macos; lib=libsplinely_runtime.dylib ;;
  MINGW* | MSYS* | CYGWIN*) echo "Windows: run scripts/package-release.ps1 instead" >&2; exit 2 ;;
  *) echo "unsupported OS: $os" >&2; exit 2 ;;
esac
no_build=0
[ "${1:-}" = "--no-build" ] && no_build=1

sha256() { # sha256 <file> -> "<hash>  <name>" in `sha256sum -c` format
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi
}

# Rust triple -> arch name used in the archive name.
arch_of() { case "$1" in aarch64-*) echo aarch64 ;; x86_64-*) echo x86_64 ;; *) echo "${1%%-*}" ;; esac; }

build() { # build [triple]  (prints nothing; binaries land in $(release_dir triple))
  if [ -n "${1:-}" ]; then
    cargo build --release --locked --workspace --target "$1"
  else
    cargo build --release --locked --workspace
  fi
}
release_dir() { local t=${CARGO_TARGET_DIR:-target}; if [ -n "${1:-}" ]; then echo "$t/$1/release"; else echo "$t/release"; fi; }

# Ad-hoc sign (no Developer ID, no notarization). arm64 macOS refuses to run unsigned code;
# Rust's linker already signs arm64 output, but lipo/strip invalidate signatures, so redo it.
adhoc_sign() {
  [ "$os_name" = macos ] || return 0
  for f in "$@"; do codesign --force --sign - "$f"; done
}

stage_common() { # stage_common <stage dir>
  cp -r LICENSE README*.md llms.txt LLM.md CLI.md RUNTIME_API.md CORE_API.md docs/INSTALL-BINARIES.md prompts examples "$1/"
  rm -rf "$1/examples/native-runtime/__pycache__"
  install -m 644 crates/splinely_runtime/include/splinely_runtime.h "$1/include/"
}

pack() { # pack <name>   (stage dir dist/<name> must be complete)
  local name=$1
  rm -f "dist/$name.tar.gz" "dist/$name.tar.gz.sha256"
  tar -C dist -czf "dist/$name.tar.gz" "$name"
  (cd dist && sha256 "$name.tar.gz" > "$name.tar.gz.sha256")
  echo "dist/$name.tar.gz"
}

bins="splinely splinely-cli splinely-viewer splinely-runtime"

if [ "${SPLINELY_UNIVERSAL:-0}" = 1 ]; then
  [ "$os_name" = macos ] || { echo "SPLINELY_UNIVERSAL is macOS only" >&2; exit 2; }
  name="splinely-v${version}-universal-macos"
  if [ "$no_build" = 0 ]; then build aarch64-apple-darwin; build x86_64-apple-darwin; fi
  stage="dist/$name"
  rm -rf "$stage"
  mkdir -p "$stage/bin" "$stage/lib" "$stage/include"
  for bin in $bins; do
    lipo -create "${CARGO_TARGET_DIR:-target}/aarch64-apple-darwin/release/$bin" "${CARGO_TARGET_DIR:-target}/x86_64-apple-darwin/release/$bin" -output "$stage/bin/$bin"
    chmod 755 "$stage/bin/$bin"
  done
  lipo -create "${CARGO_TARGET_DIR:-target}/aarch64-apple-darwin/release/$lib" "${CARGO_TARGET_DIR:-target}/x86_64-apple-darwin/release/$lib" -output "$stage/lib/$lib"
  adhoc_sign "$stage"/bin/* "$stage/lib/$lib"
  stage_common "$stage"
  pack "$name"
  exit 0
fi

target="${SPLINELY_TARGET:-}"
if [ -n "$target" ]; then arch=$(arch_of "$target"); else arch=$(uname -m); fi
[ "$arch" = arm64 ] && arch=aarch64
name="splinely-v${version}-${arch}-${os_name}"
[ "$no_build" = 1 ] || build "$target"
out=$(release_dir "$target")
stage="dist/$name"
rm -rf "$stage"
mkdir -p "$stage/bin" "$stage/lib" "$stage/include"
for bin in $bins; do
  install -m 755 "$out/$bin" "$stage/bin/"
done
install -m 755 "$out/$lib" "$stage/lib/"
adhoc_sign "$stage"/bin/* "$stage/lib/$lib"
stage_common "$stage"
pack "$name"
