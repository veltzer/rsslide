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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_languages_return_svg() {
        for lang in &["rust", "python", "javascript", "js", "typescript", "ts",
                      "go", "golang", "java", "cpp", "c++", "ruby", "rb",
                      "bash", "sh", "shell", "html", "css"] {
            let svg = language_icon(lang);
            assert!(svg.is_some(), "expected icon for '{lang}'");
            // Every bundled file should be valid UTF-8 SVG
            let content = svg.unwrap();
            assert!(content.contains("<svg"), "icon for '{lang}' does not look like SVG");
        }
    }

    #[test]
    fn unknown_language_returns_none() {
        assert!(language_icon("cobol").is_none());
        assert!(language_icon("").is_none());
        assert!(language_icon("RUST").is_none()); // case-sensitive
    }

    #[test]
    fn aliases_return_same_content() {
        assert_eq!(language_icon("javascript"), language_icon("js"));
        assert_eq!(language_icon("typescript"), language_icon("ts"));
        assert_eq!(language_icon("go"),         language_icon("golang"));
        assert_eq!(language_icon("ruby"),        language_icon("rb"));
        assert_eq!(language_icon("bash"),        language_icon("sh"));
        assert_eq!(language_icon("bash"),        language_icon("shell"));
        assert_eq!(language_icon("cpp"),         language_icon("c++"));
    }
}
