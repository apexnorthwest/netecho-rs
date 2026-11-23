#!/bin/bash -eu

cd "$SRC/netecho-rs"
cargo metadata --manifest-path fuzz/Cargo.toml --locked --format-version 1 > /dev/null
cargo fuzz build -O --debug-assertions
cp fuzz/target/x86_64-unknown-linux-gnu/release/socket_http "$OUT/socket_http"
zip -j "$OUT/socket_http_seed_corpus.zip" fuzz/seeds/socket_http/*
cp fuzz/socket_http.options "$OUT/socket_http.options"
