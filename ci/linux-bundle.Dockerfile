# Builds the Linux packages (.deb / .rpm) from Windows or any Docker host.
#   docker build -t island-linux -f ci/linux.Dockerfile ci
#   docker build -t island-linux-bundle -f ci/linux-bundle.Dockerfile ci
#   docker run --rm -v "$PWD:/src" -v island-cargo:/usr/local/cargo/registry -v island-target:/target \
#     -v "$PWD/dist-linux:/out" island-linux-bundle bash /src/ci/linux-bundle.sh
# Requires the frontend already built (npm run build → dist/).
FROM island-linux
RUN cargo install tauri-cli --version "^2" --locked
