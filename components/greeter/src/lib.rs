//! Greets a name, the default name when it's empty, and logs each greeting.

use componentized::build::defaults;
use exports::componentized::build::greeter::Guest;
use wasi::logging::logging::{Level, log};

struct Component;

impl Guest for Component {
    fn greet(name: String) -> String {
        let greeting = greeting(&name, defaults::name);
        log(Level::Info, "greeter", &greeting);
        greeting
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
    // `path` is relative to the manifest, `components/greeter/`
    path: "../wit",
    world: "greeter",
    generate_all,
});

export!(Component);
