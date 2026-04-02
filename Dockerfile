FROM rust:1.88.0-bookworm AS builder
WORKDIR /app
COPY . .
RUN apt-get update -y && apt-get install -y libclang-dev protobuf-compiler
ENV RUSTFLAGS="-C force-frame-pointers=yes -C debuginfo=1 --cfg tokio_unstable"
RUN SKIP_GUEST_BUILD=1 cargo build --release


FROM ubuntu:22.04
RUN apt-get update && apt-get install -y ca-certificates wget libjemalloc-dev graphviz binutils ghostscript
WORKDIR /app
COPY --from=builder /app/target/release/citrea .
COPY --from=builder /app/target/release/citrea-cli .
ENTRYPOINT  ["/app/citrea"]