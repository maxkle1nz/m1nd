FROM rust:1.82-slim AS builder
WORKDIR /build
COPY . .
RUN cargo build --release --features tier1,tier2

FROM debian:bookworm-slim
COPY --from=builder /build/target/release/m1nd-mcp /usr/local/bin/m1nd-mcp
ENTRYPOINT ["m1nd-mcp"]
