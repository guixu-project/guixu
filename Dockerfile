ARG REGISTRY=""
FROM ${REGISTRY}rust:1.85-bookworm AS builder
WORKDIR /src

# Use USTC crates.io mirror if official registry is slow
RUN if ! curl -s --connect-timeout 3 https://crates.io >/dev/null 2>&1; then \
      mkdir -p /usr/local/cargo/registry && \
      printf '[source.crates-io]\nreplace-with = "ustc"\n[source.ustc]\nregistry = "sparse+https://mirrors.ustc.edu.cn/crates.io-index/"\n' \
        > /usr/local/cargo/.cargo/config.toml; \
    fi

RUN apt-get update && apt-get install -y libclang-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY demo-ui demo-ui
RUN cargo build --release

FROM ${REGISTRY}debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/data-node /usr/local/bin/guixu
EXPOSE 3927
ENTRYPOINT ["guixu"]
CMD ["start"]
