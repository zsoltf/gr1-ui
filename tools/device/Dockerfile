FROM rust:1.90.0-bookworm@sha256:3914072ca0c3b8aad871db9169a651ccfce30cf58303e5d6f2db16d1d8a7e58f
RUN apt-get update && apt-get install -y --no-install-recommends \
    gcc-arm-linux-gnueabihf g++-arm-linux-gnueabihf binutils-arm-linux-gnueabihf \
    cmake make pkg-config python3 python3-pyelftools file \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add armv7-unknown-linux-gnueabihf
WORKDIR /work
