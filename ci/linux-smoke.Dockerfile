# Adds a virtual screen and tools for ci/linux-smoke.sh on top of the build image.
#   docker build -t island-linux-smoke -f ci/linux-smoke.Dockerfile ci
FROM island-linux
RUN apt-get update && apt-get install -y --no-install-recommends \
      xvfb dbus dbus-x11 imagemagick procps fonts-dejavu-core \
    && rm -rf /var/lib/apt/lists/*
