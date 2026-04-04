use std::sync::OnceLock;

use tree_sitter::Query;

/// Tree-sitter query for JavaScript symbol extraction.
///
/// Subset of the TypeScript query: no interfaces, no type aliases,
/// no type annotations. JavaScript files are parsed with the TSX grammar
/// (see `parse.rs`), so node types are compatible.
const JAVASCRIPT_QUERY_SOURCE: &str = r#"
; --- Function declarations ---
(function_declaration
  name: (identifier) @func_name) @function_decl

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

; --- Method definitions ---
(method_definition
  name: (property_identifier) @method_name) @method_def

; --- Enum declarations (yes, JS has them in some transpiled code patterns) ---
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

static JAVASCRIPT_QUERY: OnceLock<Query> = OnceLock::new();

/// Get the compiled JavaScript symbol extraction query.
pub fn javascript_query() -> &'static Query {
    JAVASCRIPT_QUERY.get_or_init(|| {
        Query::new(&crate::language_tsx(), JAVASCRIPT_QUERY_SOURCE)
            .expect("JavaScript query compilation failed — this is a build bug")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn javascript_query_compiles() {
        let query = javascript_query();
        assert!(!query.capture_names().is_empty());
    }

    #[test]
    fn javascript_query_lacks_interface_and_type_alias() {
        let query = javascript_query();
        let names: Vec<&str> = query.capture_names().to_vec();
        assert!(
            !names.contains(&"interface_name"),
            "JS query should not capture interfaces"
        );
        assert!(
            !names.contains(&"type_alias_name"),
            "JS query should not capture type aliases"
        );
    }
}
