# test-hello

A greeter that logs each greeting to stderr, e.g. `Hello, world!`.

Composes the [`test-greeter`](../test-greeter/) with its [`test-defaults`](../test-defaults/), and with [`to-stderr`](../dep-componentized-logging-to-stderr/) from componentized logging for `wasi:logging`. The composition exports [`componentized:build/greeter`](../../wit/worlds.wit) and imports `wasi:cli/stderr`.

```sh
wasmtime run --invoke 'greet("build")' test-hello.wasm
```
