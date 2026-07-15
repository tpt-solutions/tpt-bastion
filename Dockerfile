# SPDX-License-Identifier: Apache-2.0 OR MIT

# Multi-stage build for the `bastion` CLI. The release binary is copied into a
# minimal distroless base image so the published image carries no build tools.
#
#   docker build -t ghcr.io/tpt/bastion:latest .
#   docker run --rm ghcr.io/tpt/bastion:latest --help

FROM rust:1-slim AS builder
WORKDIR /usr/src/bastion
COPY . .
RUN cargo build --release --bin bastion && strip target/release/bastion

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=builder /usr/src/bastion/target/release/bastion /usr/local/bin/bastion
ENTRYPOINT ["/usr/local/bin/bastion"]
