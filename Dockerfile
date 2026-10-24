# syntax=docker/dockerfile:1.7
#
# Local build:   docker build -t blockheads-server .
# Run:           docker run -i --rm -p 15151:15151/udp -v bh-data:/data blockheads-server --new MYWORLD
#
# Release CI builds the binaries natively per architecture and uses the `release` target, which only copies
# them (no emulated compilation): docker build --target release  (expects docker-bin/<TARGETPLATFORM>/blockheads_server)

FROM --platform=$BUILDPLATFORM busybox:stable AS prep
RUN mkdir -p /data

FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock rustfmt.toml ./
COPY .cargo .cargo
COPY crates crates
RUN cargo build --release --locked -p blockheads_server --bin blockheads_server \
    && strip target/release/blockheads_server

# The image is built WITHOUT the `jvm` feature (no JRE inside; JVM mods need the CLI archive or a host with Java).
#
# Small runtime: glibc, libgcc and libstdc++ (distroless/cc ships libstdc++.so.6; Luau, the mod runtime, is C++ and
# links it dynamically), non-root (uid 65532).
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime-base
ENV BLOCKHEADS_DATA_DIR=/data
COPY --from=prep --chown=65532:65532 /data /data
VOLUME /data
WORKDIR /data
EXPOSE 15151/udp
ENTRYPOINT ["/usr/local/bin/blockheads_server"]
CMD ["--help"]

FROM runtime-base AS release
ARG TARGETPLATFORM
COPY docker-bin/${TARGETPLATFORM}/blockheads_server /usr/local/bin/blockheads_server

FROM runtime-base AS final
COPY --from=build /src/target/release/blockheads_server /usr/local/bin/blockheads_server
