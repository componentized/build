# greeter-wasip2

Greets a name, with the default name from the environment, `GREETER_NAME`, when the name is empty, built for `wasm32-wasip2`.

Implements [`componentized:build/greeter`](../../wit/worlds.wit), the [`env-greeter`](../wit/worlds.wit) world. Reads the environment through the standard library, which imports `wasi:cli` from WASI 0.2 and links the wasi-libc the Rust toolchain bundles. It exists to smoke test building, and generating the SBOM of, a component for `wasm32-wasip2`.
