#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, strum::Display)]
pub enum Action {
    Added,
    Removed,
    Changed,
}

pub fn render_list<S>(items: &[(S, Action)]) -> String
where
    S: std::fmt::Display,
{
    use std::fmt::Write as _;
    let mut result = String::new();
    for (path, item) in items {
        writeln!(&mut result, "{}: {}", item, path).expect("Unable to write to string");
    }
    result
}
