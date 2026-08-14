#!/usr/bin/env bash
# Download a Whisper ggml model for Rustle.
# Usage: ./download-model.sh [model]   (default: base.en)
# Good choices: base.en (fast, for the spike), large-v3 (best quality, ~3 GB).
set -euo pipefail

MODEL="${1:-base.en}"
DEST="models/ggml-${MODEL}.bin"
URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-${MODEL}.bin"

mkdir -p models
echo "Downloading ggml-${MODEL}.bin ..."
curl -L --fail --progress-bar -o "${DEST}" "${URL}"
echo "Saved to ${DEST}"
echo "Run with:  RUSTLE_MODEL=${DEST} cargo run"
