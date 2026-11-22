#!/bin/sh
# Copies the canonical openapi.yaml into the gateway image's docs folder.
# Run after every contract change; `cargo test` fails while the two differ.
set -eu
cd "$(dirname "$0")/.."
cp openapi.yaml gateway/docs/openapi.yaml
echo "gateway/docs/openapi.yaml updated"
