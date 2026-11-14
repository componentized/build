# hello-plug

A greeter that logs each greeting to stderr, like [`hello`](../hello/), plugged together with `wac plug`.

Plugs the [`greeter`](../greeter/) with its [`defaults`](../defaults/), and with [`to-stderr`](../dep-componentized-logging-to-stderr/) from componentized logging for `wasi:logging`. The plugged component exports the exports of the greeter, [`componentized:build/greeter`](../../wit/worlds.wit), and imports `wasi:cli/stderr`.

```sh
wasmtime run --invoke 'greet("build")' hello-plug.wasm
```
