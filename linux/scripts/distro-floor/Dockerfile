ARG BASE
FROM ${BASE}
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential ca-certificates curl pkg-config clang gettext git \
    libgtk-4-dev libadwaita-1-dev libgtksourceview-5-dev libglib2.0-dev-bin \
    libssl-dev libsecret-1-dev libkrb5-dev libsqlite3-dev \
    xvfb xauth dbus dbus-x11 python3 \
    && rm -rf /var/lib/apt/lists/*
ARG RUST_VERSION=1.98.0
ENV CARGO_HOME=/opt/cargo RUSTUP_HOME=/opt/rustup PATH=/opt/cargo/bin:$PATH
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain ${RUST_VERSION}
WORKDIR /src
