# hello

A greeter that logs each greeting to stderr, e.g. `Hello, world!`.

Composes the [`greeter`](../greeter/) with its [`defaults`](../defaults/), and with [`to-stderr`](../dep-componentized-logging-to-stderr/) from componentized logging for `wasi:logging`. The composition exports [`componentized:build/greeter`](../../wit/worlds.wit) and imports `wasi:cli/stderr`.

```sh
wasmtime run --invoke 'greet("build")' hello.wasm
```
