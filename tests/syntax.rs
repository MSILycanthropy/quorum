//! That highlighting lands on the right bytes with the right colours.

use std::path::Path;

use sift::diff::Source;
use sift::syntax::Syntax;

fn painted(text: &str, path: &str) -> Vec<(String, String)> {
    let source = Source::new(text);
    let highlighting = Syntax::default().highlight(&source, Path::new(path)).expect("valid source");

    (0..source.len())
        .flat_map(|index| {
            let line = source.line(index);
            highlighting
                .line(index)
                .iter()
                .map(|highlight| {
                    (
                        line[highlight.span.start..highlight.span.end].to_owned(),
                        highlight.color.to_string(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn color_of(painted: &[(String, String)], token: &str) -> String {
    painted
        .iter()
        .find(|(text, _)| text == token)
        .map_or_else(|| panic!("no span for {token:?}"), |(_, color)| color.clone())
}

#[test]
fn rust_keywords_strings_and_comments_take_their_theme_colours() {
    let painted = painted("let name = \"sift\"; // hi\n", "demo.rs");

    assert_eq!(color_of(&painted, "let"), "#9d7cd8");
    assert_eq!(color_of(&painted, "\"sift\""), "#9ece6a");
    assert_eq!(color_of(&painted, "// hi"), "#565f89");
    assert_eq!(color_of(&painted, ";"), "#89ddff");
}

#[test]
fn a_comment_marker_is_painted_with_the_comment_it_opens() {
    let painted = painted("let name = \"sift\"; // hi\n", "demo.rs");

    assert!(painted.iter().any(|(text, _)| text == "// hi"));
}

#[test]
fn spans_of_a_line_are_contiguous_and_cover_it_exactly() {
    let text = "fn main() {\n    println!(\"{}\", 42);\n}\n";
    let source = Source::new(text);
    let highlighting =
        Syntax::default().highlight(&source, Path::new("demo.rs")).expect("valid source");

    for index in 0..source.len() {
        let mut at = 0;
        for highlight in highlighting.line(index) {
            assert_eq!(highlight.span.start, at);
            at = highlight.span.end;
        }
        assert_eq!(at, source.line(index).len());
    }
}

#[test]
fn an_unknown_extension_falls_back_to_plain_text_without_failing() {
    let painted = painted("let name = \"sift\";\n", "demo.unheardof");

    assert_eq!(painted.len(), 1);
    assert_eq!(painted[0].1, "#c0caf5");
}

#[test]
fn a_file_is_highlighted_by_the_grammar_its_extension_names() {
    let painted = painted("[package]\nname = \"sift\"\n", "Cargo.toml");

    assert_eq!(color_of(&painted, "package"), "#2ac3de");
    assert_eq!(color_of(&painted, "\"sift\""), "#9ece6a");
}

#[test]
fn a_file_without_an_extension_is_highlighted_by_its_shebang() {
    let painted = painted("#!/usr/bin/env bash\necho hi\n", "script");

    assert_eq!(color_of(&painted, "#!/usr/bin/env bash"), "#565f89");
    assert_eq!(color_of(&painted, "echo"), "#7aa2f7");
}

#[test]
fn a_yml_file_is_highlighted_as_yaml() {
    let painted = painted("name: sift\n", "config.yml");

    assert_ne!(color_of(&painted, "name"), "#c0caf5");
}

#[test]
fn an_erb_template_paints_its_ruby_and_its_html() {
    let painted = painted("<p><%= user.name if admin? %></p>\n", "show.html.erb");

    assert_eq!(color_of(&painted, "<%="), "#9d7cd8");
    assert_ne!(color_of(&painted, "if"), "#c0caf5");
    assert_ne!(color_of(&painted, "p"), "#c0caf5");
}

#[test]
fn a_gemfile_is_highlighted_as_ruby() {
    let painted = painted("gem \"rails\"\n", "Gemfile");

    assert_eq!(color_of(&painted, "\"rails\""), "#9ece6a");
}
