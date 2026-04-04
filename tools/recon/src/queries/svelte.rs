use std::sync::OnceLock;

use tree_sitter::{Language, Query};

/// Tree-sitter query for Svelte 5 rune detection in re-parsed script blocks.
///
/// Runes (`$state()`, `$derived()`, `$effect()`, `$props()`, `$bindable()`)
/// parse as regular `call_expression` nodes in the TypeScript grammar.
/// We match them by function name.
const RUNE_QUERY_SOURCE: &str = r#"
(call_expression
  function: (identifier) @rune_name
  (#match? @rune_name "^\\$(state|derived|effect|props|bindable)$")) @rune_call
"#;

/// Query for Svelte 4 legacy patterns: `export let` for props.
///
/// In Svelte 4, component props are declared as `export let propName`.
/// The tree-sitter TypeScript grammar parses these as export statements
/// containing lexical declarations.
const LEGACY_PROP_QUERY_SOURCE: &str = r#"
(export_statement
  (lexical_declaration
    (variable_declarator
      name: (identifier) @legacy_prop_name))) @legacy_prop
"#;

static RUNE_QUERY: OnceLock<Query> = OnceLock::new();
static LEGACY_PROP_QUERY: OnceLock<Query> = OnceLock::new();

/// Get the compiled Svelte rune detection query.
///
/// This runs against TS-parsed script blocks, not the Svelte tree.
pub fn rune_query(language: &Language) -> &'static Query {
    RUNE_QUERY.get_or_init(|| {
        Query::new(language, RUNE_QUERY_SOURCE)
            .expect("Svelte rune query compilation failed — this is a build bug")
    })
}

/// Get the compiled Svelte 4 legacy prop query.
pub fn legacy_prop_query(language: &Language) -> &'static Query {
    LEGACY_PROP_QUERY.get_or_init(|| {
        Query::new(language, LEGACY_PROP_QUERY_SOURCE)
            .expect("Svelte legacy prop query compilation failed — this is a build bug")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rune_query_compiles() {
        let lang = crate::language_typescript();
        let query = rune_query(&lang);
        let names: Vec<&str> = query.capture_names().to_vec();
        assert!(names.contains(&"rune_name"));
        assert!(names.contains(&"rune_call"));
    }

    #[test]
    fn legacy_prop_query_compiles() {
        let lang = crate::language_typescript();
        let query = legacy_prop_query(&lang);
        let names: Vec<&str> = query.capture_names().to_vec();
        assert!(names.contains(&"legacy_prop_name"));
    }
}
