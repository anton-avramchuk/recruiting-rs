# syntax=docker/dockerfile:1.7
#
# Сборка всей платформы. Цели (--target):
#   identity | admin | candidate | recruitment | bff-crm | bff-admin | web-crm | web-admin
# Пример: docker compose --profile app up -d --build

# ---------- общая база сборки ----------
FROM rust:1-bookworm AS toolchain
WORKDIR /src
RUN rustup target add wasm32-unknown-unknown \
 && curl -fsSL https://github.com/leptos-rs/cargo-leptos/releases/download/v0.3.11/cargo-leptos-x86_64-unknown-linux-gnu.tar.gz -o /tmp/cl.tgz \
 && mkdir /tmp/cl && tar -xzf /tmp/cl.tgz -C /tmp/cl \
 && install "$(find /tmp/cl -name cargo-leptos -type f | head -n1)" /usr/local/bin/cargo-leptos \
 && rm -rf /tmp/cl /tmp/cl.tgz

# ---------- бэкенд-сервисы ----------
FROM toolchain AS backend-build
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p identity -p admin -p candidate -p recruitment -p bff-crm -p bff-admin \
 && mkdir /out \
 && cp target/release/identity target/release/admin target/release/candidate target/release/recruitment \
       target/release/bff-crm target/release/bff-admin /out/

# ---------- веб-приложения (SSR-бинарник + wasm-пакет) ----------
FROM toolchain AS web-build
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo leptos build --release --project web-crm \
 && cargo leptos build --release --project web-admin \
 && mkdir -p /out/web-crm /out/web-admin \
 && cp target/release/web-crm /out/web-crm/web-crm \
 && cp -r target/site/web-crm /out/web-crm/site \
 && cp target/release/web-admin /out/web-admin/web-admin \
 && cp -r target/site/web-admin /out/web-admin/site

# ---------- рантайм ----------
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --no-create-home app
USER app
ENV LISTEN_ADDR=0.0.0.0:8080
EXPOSE 8080

FROM runtime AS identity
COPY --from=backend-build /out/identity /usr/local/bin/app
CMD ["app"]

FROM runtime AS admin
COPY --from=backend-build /out/admin /usr/local/bin/app
CMD ["app"]

FROM runtime AS candidate
COPY --from=backend-build /out/candidate /usr/local/bin/app
CMD ["app"]

FROM runtime AS recruitment
COPY --from=backend-build /out/recruitment /usr/local/bin/app
CMD ["app"]

FROM runtime AS bff-crm
COPY --from=backend-build /out/bff-crm /usr/local/bin/app
CMD ["app"]

FROM runtime AS bff-admin
COPY --from=backend-build /out/bff-admin /usr/local/bin/app
CMD ["app"]

FROM runtime AS web-crm
COPY --from=web-build /out/web-crm/web-crm /usr/local/bin/app
COPY --from=web-build /out/web-crm/site /app/site
ENV LEPTOS_OUTPUT_NAME=web-crm LEPTOS_SITE_ROOT=/app/site LEPTOS_SITE_PKG_DIR=pkg \
    LEPTOS_SITE_ADDR=0.0.0.0:8080 LEPTOS_ENV=PROD
CMD ["app"]

FROM runtime AS web-admin
COPY --from=web-build /out/web-admin/web-admin /usr/local/bin/app
COPY --from=web-build /out/web-admin/site /app/site
ENV LEPTOS_OUTPUT_NAME=web-admin LEPTOS_SITE_ROOT=/app/site LEPTOS_SITE_PKG_DIR=pkg \
    LEPTOS_SITE_ADDR=0.0.0.0:8080 LEPTOS_ENV=PROD
CMD ["app"]
