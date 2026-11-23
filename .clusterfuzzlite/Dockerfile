FROM gcr.io/oss-fuzz-base/base-builder-rust
COPY . $SRC/netecho-rs
WORKDIR $SRC/netecho-rs
COPY .clusterfuzzlite/build.sh $SRC/build.sh
