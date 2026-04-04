/// Returns the SVG source for a well-known programming language icon,
/// or `None` if no icon is bundled for the given syntect language token.
pub fn language_icon(token: &str) -> Option<&'static str> {
    match token {
        "rust" => Some(include_str!("../assets/icons/rust.svg")),
        "python" => Some(include_str!("../assets/icons/python.svg")),
        "javascript" | "js" => Some(include_str!("../assets/icons/javascript.svg")),
        "typescript" | "ts" => Some(include_str!("../assets/icons/typescript.svg")),
        "go" | "golang" => Some(include_str!("../assets/icons/go.svg")),
        "java" => Some(include_str!("../assets/icons/openjdk.svg")),
        "cpp" | "c++" => Some(include_str!("../assets/icons/cplusplus.svg")),
        "ruby" | "rb" => Some(include_str!("../assets/icons/ruby.svg")),
        "bash" | "sh" | "shell" => Some(include_str!("../assets/icons/gnubash.svg")),
        "html" => Some(include_str!("../assets/icons/html5.svg")),
        "css" => Some(include_str!("../assets/icons/css3.svg")),
        _ => None,
    }
}
