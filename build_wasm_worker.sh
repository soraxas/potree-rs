#!/bin/sh

set -ex

# Build std with atomics and shared memory support enabled.
# This is required by wasm_thread for multi-threading via Web Workers.

RUSTFLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals \
  -C link-arg=--shared-memory \
  -C link-arg=--max-memory=1073741824 \
  -C link-arg=--import-memory \
  -C link-arg=--export=__wasm_init_tls \
  -C link-arg=--export=__tls_size \
  -C link-arg=--export=__tls_align \
  -C link-arg=--export=__tls_base \
  --cfg getrandom_backend=\"wasm_js\"" \
  cargo +nightly build \
      --features="wasm_worker" \
      --example read_wasm \
      --target wasm32-unknown-unknown \
      --profile wasm-release \
      -Z build-std=std,panic_abort

# Note the usage of `--target no-modules` here which is required for passing
# the memory import to each wasm module.
wasm-bindgen \
  target/wasm32-unknown-unknown/wasm-release/examples/read_wasm.wasm \
  --out-dir ./wasm \
  --target web
