# Potree file parser in RUST

Features / Roadmap:

- [x] Load data asynchronously
- [x] Load (asynchronously) and parse hierarchy (lazy & entire) from filesystem or http
- [x] Native & WASM compatibility
- [x] WASM Multithread compatibility (using SharedArrayBuffer and specific http headers)
- [x] Provide a simple slab implementation to load points progressively
- [-] Load points (in progress, not all attributes are loaded for the moment)
- [ ] Make datasource customizable (`ResourceLoader` should be a trait)

## Download sample potree file

Go in the `assets/heidentor` folder and run `dl.sh` script:

```bash
cd assets/heidentor/
./dl.sh
```

## Run the read native example

There is multiple native examples:

### Load points from http

This example loads points from an http url source using ehttp client:

```bash
cargo run --features="ehttp" --example read_native_http
```

### Load points from http

This example loads points from an http url source using ehttp client:

```bash
cargo run --features="ehttp" --example read_native_http
```

### Load points from local filesystem

This example loads points from an http url source using ehttp client:

```bash
cargo run --features="fs" --example read_native_fs
```

### Load points from local filesystem using a slab

This example loads points from an http url source using ehttp client:

```bash
cargo run --features="fs" --example read_native_slab
```

## Build WASM example

This example loads a potree point cloud in the browser in the main thread.

To prevent the worker to terminate and not executing async tasks, the example uses the hack mentionned in this issue: https://github.com/rustwasm/wasm-bindgen/issues/2945.

To build the example, comment the tokio dev dependency from `Cargo.toml`.

Then, build using the provided script: (install the required rust nightly if asked)

```bash
./build_wasm.sh
```

## Build WASM multithreaded example

This example uses a webworker for parsing, and delegates the http requests to the main thread (using provided `EhttpClientLocal`).

To prevent the worker to terminate and not executing async tasks, the example uses the hack mentionned in this issue: https://github.com/rustwasm/wasm-bindgen/issues/2945.

To build the example, comment the tokio dev dependency from `Cargo.toml`.

Then, build using the provided script: (install the required rust nightly if asked)

```bash
./build_wasm_worker.sh
```

## Run WASM simple or multithreaded example

Install express and run `serve.js`:

Note: this server sends the security headers to allow using workers in WASM.

```bash
npm install express
node serve.js
```

Open the browser at address http://localhost:8080/wasm/ and check network / console panels to see the requests / logs.


## Credits

- Potree file format has been created by Markus Schütz, see [Potree](https://github.com/potree/potree)
  Copyright (c) 2011-2020, Markus Schütz  
  Licensed under the BSD 2-Clause License (see THIRD_PARTY_LICENSES.md).
