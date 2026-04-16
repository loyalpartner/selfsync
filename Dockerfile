FROM rust:slim AS builder

RUN apt-get update && apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .
RUN cargo build --release -p selfsync-server

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    libsqlite3-0 ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/selfsync-server /usr/local/bin/

ENV SELFSYNC_DB=/data/selfsync.db
ENV SELFSYNC_ADDR=0.0.0.0:8080

VOLUME /data
EXPOSE 8080

ENTRYPOINT ["selfsync-server"]
