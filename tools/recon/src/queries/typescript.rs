use std::sync::OnceLock;

use tree_sitter::Query;

/// Tree-sitter query for TypeScript/TSX symbol extraction.
///
/// Captures:
/// - Function declarations (named, async, generator)
/// - Arrow functions assigned to variables (const/let/var)
/// - Class declarations (abstract, decorated, with heritage)
/// - Method definitions (static, async, getters/setters, visibility)
/// - Interface declarations (with extends)
/// - Type aliases
/// - Enums (including const enums)
/// - Import statements (named, default, namespace, side-effect, type-only, dynamic)
/// - Export statements (named, default, re-exports, star re-exports, type-only)
const TYPESCRIPT_QUERY_SOURCE: &str = r#"
; --- Function declarations ---
(function_declaration
  name: (identifier) @func_name) @function_decl

; Async/generator function declarations
(generator_function_declaration
  name: (identifier) @gen_func_name) @generator_function_decl

; --- Arrow functions / function expressions assigned to variables ---
(lexical_declaration
  (variable_declarator
    name: (identifier) @arrow_name
    value: (arrow_function) @arrow_value)) @arrow_decl

(lexical_declaration
  (variable_declarator
    name: (identifier) @func_expr_name
    value: (function_expression) @func_expr_value)) @func_expr_decl

(variable_declaration
  (variable_declarator
    name: (identifier) @var_arrow_name
    value: (arrow_function) @var_arrow_value)) @var_arrow_decl

(variable_declaration
  (variable_declarator
    name: (identifier) @var_func_expr_name
    value: (function_expression) @var_func_expr_value)) @var_func_expr_decl

; --- Class declarations ---
(class_declaration
  name: (type_identifier) @class_name) @class_decl

(abstract_class_declaration
  name: (type_identifier) @abstract_class_name) @abstract_class_decl

; --- Method definitions ---
(method_definition
  name: (property_identifier) @method_name) @method_def

; --- Interface declarations ---
(interface_declaration
  name: (type_identifier) @interface_name) @interface_decl

; --- Type aliases ---
(type_alias_declaration
  name: (type_identifier) @type_alias_name) @type_alias_decl

; --- Enum declarations ---
(enum_declaration
  name: (identifier) @enum_name) @enum_decl

; --- Import statements ---
(import_statement
  source: (string) @import_source) @import_stmt

; --- Dynamic imports ---
(call_expression
  function: (import)
  arguments: (arguments (string) @dynamic_import_source)) @dynamic_import

; --- Export statements ---
(export_statement) @export_stmt
"#;

static TYPESCRIPT_QUERY: OnceLock<Query> = OnceLock::new();
static TSX_QUERY: OnceLock<Query> = OnceLock::new();

/// Get the compiled TypeScript symbol extraction query.
pub fn typescript_query() -> &'static Query {
    TYPESCRIPT_QUERY.get_or_init(|| {
        Query::new(&crate::language_typescript(), TYPESCRIPT_QUERY_SOURCE)
            .expect("TypeScript query compilation failed — this is a build bug")
    })
}

/// Get the compiled TSX symbol extraction query.
pub fn tsx_query() -> &'static Query {
    TSX_QUERY.get_or_init(|| {
        Query::new(&crate::language_tsx(), TYPESCRIPT_QUERY_SOURCE)
            .expect("TypeScript query compilation failed — this is a build bug")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typescript_query_compiles() {
        let query = typescript_query();
        assert!(!query.capture_names().is_empty());
    }

    #[test]
    fn typescript_query_has_expected_captures() {
        let query = typescript_query();
        let names: Vec<&str> = query.capture_names().to_vec();
        assert!(names.contains(&"func_name"), "missing func_name capture");
        assert!(names.contains(&"class_name"), "missing class_name capture");
        assert!(
            names.contains(&"interface_name"),
            "missing interface_name capture"
        );
        assert!(
            names.contains(&"type_alias_name"),
            "missing type_alias_name capture"
        );
        assert!(names.contains(&"enum_name"), "missing enum_name capture");
        assert!(
            names.contains(&"import_source"),
            "missing import_source capture"
        );
        assert!(
            names.contains(&"export_stmt"),
            "missing export_stmt capture"
        );
    }

    #[test]
    fn tsx_query_compiles_with_tsx_grammar() {
        let query = tsx_query();
        assert!(!query.capture_names().is_empty());
    }
}
