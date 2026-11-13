# Build environment for the Linux version (Debian 12, same libs as Fedora/Debian desktops).
#   docker build -t island-linux -f ci/linux.Dockerfile ci
#   docker run --rm -v "$PWD:/src" -v island-cargo:/usr/local/cargo/registry -v island-target:/target \
#     -w /src/src-tauri -e CARGO_TARGET_DIR=/target island-linux cargo build --release
FROM rust:1-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
      libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev \
      libxdo-dev libssl-dev pkg-config build-essential file \
    && rm -rf /var/lib/apt/lists/*
