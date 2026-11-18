# test-greeter-wasip3

Greets a name, with the default name from the environment, `GREETER_NAME`, when the name is empty, built for `wasm32-wasip3`.

Implements [`componentized:build/greeter`](../../wit/worlds.wit), the [`env-greeter`](../wit/worlds.wit) world. Reads the environment and greets on a thread of its own, a component model cooperative thread, through the standard library, which links the wasi-libc of the wasi-sdk sysroot and imports WASI 0.3. It exists to smoke test building, and generating the SBOM of, a component for `wasm32-wasip3`.
