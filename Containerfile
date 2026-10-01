# rust @ 1.98-slim
FROM docker.io/library/rust@sha256:f47a8de237dcbb0b0ce1099901e60a89728e3d51f24e664b40e947171538ade7 AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src src
# Static builds require us to specify the target arch, so we have to get a little specific
# In practice, this is just a clippy run and a normal static build with auditable enabled.
# ARM64 builds use musl for static linking, as glibc static only works on AMD64 right now.
# This has performance implications for arm64 builds, as musl has some known limiations for this type of application.
# In particular, certain I/O and networking operations may be slower compared to glibc-based builds.
# As a result, you will find that arm builds do not sustain the same speeds as their amd64 counterparts.
# At some point, we will switch just arm64 to distroless to allow glibc builds at the cost of larger images.
RUN set -e ; if lscpu | grep -q x86_64; then \
    rustup component add clippy &&\
    cargo install cargo-auditable &&\
    CARGO_BUILD_WARNINGS=deny RUSTFLAGS="-C target-feature=+crt-static" cargo auditable clippy --release --target x86_64-unknown-linux-gnu &&\
    CARGO_BUILD_WARNINGS=deny RUSTFLAGS="-C target-feature=+crt-static" cargo auditable build --target x86_64-unknown-linux-gnu --release &&\
    cp target/x86_64-unknown-linux-gnu/release/netecho-rs /netecho-rs || exit 1; \
  elif lscpu | grep -q aarch64; then \
    rustup component add clippy &&\
    rustup target add aarch64-unknown-linux-musl &&\
    cargo install cargo-auditable &&\
    CARGO_BUILD_WARNINGS=deny RUSTFLAGS="-C target-feature=+crt-static" cargo auditable clippy --release --target aarch64-unknown-linux-musl &&\
    CARGO_BUILD_WARNINGS=deny RUSTFLAGS="-C target-feature=+crt-static" cargo auditable build --target aarch64-unknown-linux-musl --release &&\
    cp target/aarch64-unknown-linux-musl/release/netecho-rs /netecho-rs; \
  else \
    echo "Unsupported architecture: $(lscpu | grep Architecture | awk '{print $2}')"; \
    exit 1; \
  fi ; \
  rm -rf target # This speeds up the layer commit and thus the build

FROM scratch AS release
# Copy the binary
COPY --from=build /netecho-rs /netecho-rs
ENTRYPOINT ["/netecho-rs"]
