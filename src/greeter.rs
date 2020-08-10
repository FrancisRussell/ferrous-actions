pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greet_test() {
        assert_eq!(greet("world"), "Hello, world!");
    }
}
