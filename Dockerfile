FROM rust:1.90-slim AS builder
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
# amassada-core has a path dependency on fondament-core (../../Fondament). Provide the
# Fondament repo as a named build context so the relative path resolves to /Fondament:
#   docker build --build-context fondament=../Fondament -t ... .
WORKDIR /Fondament
COPY --from=fondament . .
WORKDIR /app
COPY . .
RUN cargo build --release --bin amassada-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/amassada-server /usr/local/bin/amassada-server
# Canvases baked in at /canvases/stdlib — no runtime volume needed.
COPY canvases/ /canvases/
EXPOSE 7700
ENTRYPOINT ["amassada-server"]
