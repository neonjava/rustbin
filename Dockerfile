FROM rust:1-bookworm AS builder
WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
COPY api ./api
RUN cargo build --release --bin rustbin

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* && useradd --system --uid 10001 rustbin
WORKDIR /app
COPY --from=builder /app/target/release/rustbin /usr/local/bin/rustbin
COPY public ./public
USER 10001
ENV RUSTBIN_HOST=0.0.0.0 PORT=8080
EXPOSE 8080
CMD ["rustbin"]
