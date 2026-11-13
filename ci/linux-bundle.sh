#!/usr/bin/env bash
# Inside ci/linux-bundle.Dockerfile: build island-hook, then the .deb and .rpm.
set -euo pipefail
export CARGO_TARGET_DIR=/target
cd /src/src-tauri
cargo build --release -p island-hook
mkdir -p binaries
cp /target/release/island-hook "binaries/island-hook-$(rustc -vV | sed -n 's/^host: //p')"
# The frontend (dist/) is built on the host; skip the npm steps here.
cargo tauri build --bundles deb,rpm --config '{"build":{"beforeBuildCommand":""}}'
mkdir -p /out
cp /target/release/bundle/deb/*.deb /target/release/bundle/rpm/*.rpm /out/
ls -la /out
