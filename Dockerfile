FROM oapi-preimage AS oapifile

FROM rust:1.86-slim-bookworm AS builder

RUN apt update \
&&  apt install -y --no-install-recommends libssl-dev pkg-config libpq5 \
&&  rm -rf /var/lib/apt/lists/*

RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid 10001 \
    "ltzf-backend"

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY --from=oapifile /app/oapicode-rust ./oapicode

RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build -r

COPY ./.sqlx ./.sqlx
COPY ./src ./src
COPY ./migrations ./migrations
ENV SQLX_OFFLINE=true
RUN touch src/main.rs && cargo build --release

FROM busybox:latest AS runner

LABEL maintainer="Benedikt Schäfer"
LABEL description="Backend for the LTZF"
LABEL version="0.2.2"

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
            CMD curl -f "http://localhost:80" || exit 1

COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group

COPY --from=builder /usr/lib/x86_64-linux-gnu/libssl.so.* /usr/lib/
COPY --from=builder /usr/lib/x86_64-linux-gnu/libcrypto.so.* /usr/lib/
COPY --from=builder /usr/lib/x86_64-linux-gnu/libpq.so.* /usr/lib/
COPY --from=builder /usr/lib/x86_64-linux-gnu/libgcc_s.so.* /usr/lib

COPY --from=builder --chmod=0100 --chown=ltzf-backend:ltzf-backend /app/target/release/ltzf-backend /app/ltzf-backend

WORKDIR /app

USER ltzf-backend

ENTRYPOINT ["./ltzf-backend"]
