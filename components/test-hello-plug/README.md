# test-hello-plug

A greeter that logs each greeting to stderr, like [`test-hello`](../test-hello/), plugged together with `wac plug`.

Plugs the [`test-greeter`](../test-greeter/) with its [`test-defaults`](../test-defaults/), and with [`to-stderr`](../dep-componentized-logging-to-stderr/) from componentized logging for `wasi:logging`. The plugged component exports the exports of the greeter, [`componentized:build/greeter`](../../wit/worlds.wit), and imports `wasi:cli/stderr`.

```sh
wasmtime run --invoke 'greet("build")' test-hello-plug.wasm
```
