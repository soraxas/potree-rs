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

## Run the native examples

### Load points from http using ehttp + tokio

This example loads points from an http url source using ehttp client:

```bash
cargo run --features="ehttp tokio_dev" --example read_native_ehttp
```

### Load points from http using reqwest + tokio

This example loads points from an http url source using reqwest client:

```bash
cargo run --features="reqwest tokio_dev" --example read_native_reqwest
```

### Load points from local filesystem

This example loads points from local filesystem:

```bash
cargo run --features="fs tokio_dev" --example read_native_fs
```

### Load points from local filesystem using a slab structure:

This example loads point cloud structure from local filesystem and store it in a slab data structure.

Then, it loads the root node points.


```bash
cargo run --features="fs tokio_dev" --example read_native_slab
```

## Build WASM example

This example loads a potree point cloud in the browser in the main thread.

To prevent the worker to terminate and not executing async tasks, the example uses the hack mentionned in this issue: https://github.com/rustwasm/wasm-bindgen/issues/2945.

To build the example, use the provided script: (install the required rust nightly if asked)

```bash
./build_wasm.sh
```

## Build WASM multithreaded example

This example uses a webworker for parsing, and delegates the http requests to the main thread (using provided `EhttpClientLocal`).

To prevent the worker to terminate and not executing async tasks, the example uses the hack mentionned in this issue: https://github.com/rustwasm/wasm-bindgen/issues/2945.

To build the example, use the provided script: (install the required rust nightly if asked)

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
