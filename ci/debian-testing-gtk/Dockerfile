FROM debian:testing

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        curl \
        git \
        nodejs \
        pkg-config \
        libgtk-4-dev \
        libadwaita-1-dev \
        libgtksourceview-5-dev \
        libssl-dev \
        libsecret-1-dev \
        libkrb5-dev \
        libsqlite3-dev \
        clang \
        dbus-daemon \
        docker-cli \
        xvfb \
        xauth \
        at-spi2-core \
        python3-pyatspi \
    && rm -rf /var/lib/apt/lists/*

RUN pkg-config --atleast-version=4.22 gtk4 \
    && pkg-config --atleast-version=2.88 glib-2.0 \
    && pkg-config --atleast-version=1.9 libadwaita-1 \
    && pkg-config --atleast-version=5.18 gtksourceview-5
