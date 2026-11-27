# syntax=docker/dockerfile:1
# O1 — multi-stage release image (docs/o1-releases-plan.md D-O1-4).
# Bases are digest-pinned: `aura-release check` refuses unpinned bases
# (`docker base unpinned`). Refresh the digests intentionally with
# `docker buildx imagetools inspect <image>:<tag>` and a reviewed commit.
FROM rust:1.99.1-slim-bookworm@sha256:f2c004e3d2d8b547e8cf7ffbc29205385f4e71d23e53e4e383065db5e9f822cd AS build
WORKDIR /src
COPY . .
ENV CARGO_BUILD_JOBS=1
RUN cargo build --release -p aura-node --bin aurad

FROM debian:trixie-slim@sha256:8aa5a7bf5a1955bddbe7d74fcdd0b91ab30e74362a8b73af672b0481b35b44a0 AS runtime
RUN useradd --system --uid 10001 auralite
COPY --from=build /src/target/release/aurad /usr/local/bin/aurad
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/aurad"]
