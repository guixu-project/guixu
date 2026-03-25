FROM rust:1.85-bookworm AS builder
WORKDIR /src
RUN apt-get update && apt-get install -y libclang-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY demo-ui demo-ui
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/data-node /usr/local/bin/guixu
EXPOSE 3927
ENTRYPOINT ["guixu"]
CMD ["start"]
