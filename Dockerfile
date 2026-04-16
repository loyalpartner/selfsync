FROM rust:alpine AS builder

RUN apk add --no-cache musl-dev protobuf-dev

WORKDIR /src
COPY . .
RUN cargo build --release -p selfsync-server

FROM alpine:3

RUN apk add --no-cache sqlite-libs ca-certificates

COPY --from=builder /src/target/release/selfsync-server /usr/local/bin/

ENV SELFSYNC_DB=/data/selfsync.db
ENV SELFSYNC_ADDR=0.0.0.0:8080

VOLUME /data
EXPOSE 8080

ENTRYPOINT ["selfsync-server"]
