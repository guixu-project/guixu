# Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
# SPDX-License-Identifier: Apache-2.0

ARG RUST_IMAGE=rust:1.85-bookworm
ARG DEBIAN_IMAGE=debian:bookworm-slim

FROM ${RUST_IMAGE} AS builder
WORKDIR /src

# Use USTC crates.io mirror if official registry is slow
RUN if ! curl -s --connect-timeout 3 https://crates.io >/dev/null 2>&1; then \
      mkdir -p $CARGO_HOME && \
      printf '[source.crates-io]\nreplace-with = "ustc"\n[source.ustc]\nregistry = "sparse+https://mirrors.ustc.edu.cn/crates.io-index/"\n' \
        > $CARGO_HOME/config.toml; \
    fi

RUN apt-get update && apt-get install -y libclang-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY ui ui
RUN cargo build --release

FROM ${DEBIAN_IMAGE}
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/data-node /usr/local/bin/guixu
EXPOSE 3927
ENTRYPOINT ["guixu"]
CMD ["start"]
