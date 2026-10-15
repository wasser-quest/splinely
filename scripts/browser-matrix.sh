#!/usr/bin/env bash
# Browser matrix (Chromium + Firefox + WebKit via Playwright) + audible-output/latency leg.
# Thin wrapper around scripts/browser-matrix.cjs; all arguments are passed through.
#
#   bash scripts/browser-matrix.sh                       # all engines, audio leg on chromium
#   bash scripts/browser-matrix.sh --browsers chromium --audio none
#   bash scripts/browser-matrix.sh --audio chromium,firefox --audio-nonblocking
#
# Environment (all optional):
#   PLAYWRIGHT_MODULE        path of an installed `playwright` package (default: `playwright` resolved from node_modules)
#   PLAYWRIGHT_BROWSERS_PATH where `npx playwright install chromium firefox webkit` put the browsers
#   SPLINELY_SCRATCH         scratch/output root (default: ~/.cache/splinely-scratch); TMPDIR is pointed into it
#   SPLINELY_CLI             splinely-cli binary (default: target/debug/splinely-cli; built with cargo only if missing
#                            and SPLINELY_MATRIX_BUILD=1, never rebuilt implicitly)
#   SPLINELY_MATRIX_SKIP_HOST_VALIDATION=1  sets PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=1 (unsupported distro whose
#                            libraries are present anyway; the engine launch itself still decides pass or skip)
# Exit: 0 pass, 1 fail, 3 incomplete (something was skipped; skips are NOT passes — see the JSON).
set -euo pipefail
cd "$(dirname "$0")/.."
export SPLINELY_SCRATCH="${SPLINELY_SCRATCH:-$HOME/.cache/splinely-scratch}"
mkdir -p "$SPLINELY_SCRATCH/tmp"
export TMPDIR="${TMPDIR:-$SPLINELY_SCRATCH/tmp}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
default_pw=/home/user/.cache/dabei-agent-test/node_modules/playwright
if [ -z "${PLAYWRIGHT_MODULE:-}" ] && [ -d "$default_pw" ]; then export PLAYWRIGHT_MODULE="$default_pw"; fi
if [ -z "${PLAYWRIGHT_BROWSERS_PATH:-}" ] && [ -d "$HOME/.cache/ms-playwright" ]; then export PLAYWRIGHT_BROWSERS_PATH="$HOME/.cache/ms-playwright"; fi
if [ "${SPLINELY_MATRIX_SKIP_HOST_VALIDATION:-}" = 1 ]; then export PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=1; fi
cli="${SPLINELY_CLI:-target/debug/splinely-cli}"
if [ ! -x "$cli" ] && [ "${SPLINELY_MATRIX_BUILD:-}" = 1 ]; then cargo build -p splinely-cli; fi
exec node scripts/browser-matrix.cjs "$@"
