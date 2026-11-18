//! Greets a name, the default name from the environment when it's empty, built for wasm32-wasip3: the
//! greeting is made on a thread of its own, a component model cooperative thread.

use exports::componentized::build::greeter::Guest;

struct Component;

impl Guest for Component {
    fn greet(name: String) -> String {
        // joined before the export returns, a thread that outlives it isn't supported by wasi-sdk 34's
        // wasi-libc, see WebAssembly/wasi-libc#884
        std::thread::spawn(move || {
            greeting(&name, || std::env::var("GREETER_NAME").unwrap_or_else(|_| "wasip3".to_string()))
        })
        .join()
        .expect("the greeting thread panicked")
    }
}

/// `Hello, <name>!`, with the default name when the name is empty.
fn greeting(name: &str, default_name: impl FnOnce() -> String) -> String {
    if name.is_empty() {
        format!("Hello, {}!", default_name())
    } else {
        format!("Hello, {name}!")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greets_the_name() {
        assert_eq!(greeting("build", || unreachable!()), "Hello, build!");
    }

    #[test]
    fn greets_the_default_name() {
        assert_eq!(greeting("", || "world".to_string()), "Hello, world!");
    }
}

wit_bindgen::generate!({
    // `path` is relative to the manifest, `components/greeter-wasip3/`
    path: "../wit",
    world: "env-greeter",
    generate_all,
});

export!(Component);
