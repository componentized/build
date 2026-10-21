//! Prints a greeting for the name given, `world` when none is.
//!
//! ```sh
//! hello build
//! ```

fn main() {
    let name = std::env::args().nth(1);
    println!("{}", greeting(name.as_deref()));
}

/// `Hello, <name>!`, `world` without a name.
fn greeting(name: Option<&str>) -> String {
    format!("Hello, {}!", name.unwrap_or("world"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greets_the_name() {
        assert_eq!(greeting(Some("build")), "Hello, build!");
    }

    #[test]
    fn greets_the_world() {
        assert_eq!(greeting(None), "Hello, world!");
    }
}
