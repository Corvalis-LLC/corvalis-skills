use std::path::Path;

use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Query, QueryCursor, Tree};

use crate::language_typescript;
use crate::output::{DetectedLanguage, Export, ExportKind, Import, ImportKind, Symbol, SymbolKind};
use crate::parse::ParsedFile;
use crate::queries::{javascript, svelte, typescript};

/// Extracted symbols, imports, and exports from a single file.
#[derive(Debug)]
pub struct FileSymbols {
    pub symbols: Vec<Symbol>,
    pub imports: Vec<Import>,
    pub exports: Vec<Export>,
}

/// Extract all symbols, imports, and exports from a parsed file.
///
/// Dispatches to the appropriate query set based on the file's language.
/// For Svelte files, extracts symbols from re-parsed script blocks
/// and adds component-level symbols (runes, legacy props).
pub fn extract_symbols(parsed_file: &ParsedFile) -> FileSymbols {
    match parsed_file.source_file.language {
        DetectedLanguage::Svelte => extract_svelte(parsed_file),
        DetectedLanguage::JavaScript | DetectedLanguage::Jsx => {
            let query = javascript::javascript_query();
            extract_from_tree(&parsed_file.tree, parsed_file.source.as_bytes(), query, 0)
        }
        DetectedLanguage::TypeScript => extract_from_tree(
            &parsed_file.tree,
            parsed_file.source.as_bytes(),
            typescript::typescript_query(),
            0,
        ),
        DetectedLanguage::Tsx => extract_from_tree(
            &parsed_file.tree,
            parsed_file.source.as_bytes(),
            typescript::tsx_query(),
            0,
        ),
    }
}

/// Extract symbols from a Svelte file's script blocks.
fn extract_svelte(parsed_file: &ParsedFile) -> FileSymbols {
    let mut symbols = Vec::new();
    let mut imports = Vec::new();
    let mut exports = Vec::new();

    // Add a Component symbol from the filename.
    let component_name = Path::new(&parsed_file.source_file.path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();

    symbols.push(Symbol {
        name: component_name,
        kind: SymbolKind::Component,
        line: 1,
        end_line: parsed_file.source.lines().count().max(1),
        exported: true,
        signature: None,
    });

    let ts_lang = language_typescript();
    let ts_query = typescript::typescript_query();
    let rune_q = svelte::rune_query(&ts_lang);
    let legacy_prop_q = svelte::legacy_prop_query(&ts_lang);

    for block in &parsed_file.script_blocks {
        let source_bytes = block.source.as_bytes();
        let line_offset = block.start_line;

        // Standard TS symbol extraction on the script block.
        let mut block_syms = extract_from_tree(&block.tree, source_bytes, ts_query, line_offset);
        symbols.append(&mut block_syms.symbols);
        imports.append(&mut block_syms.imports);
        exports.append(&mut block_syms.exports);

        // Svelte 5 rune extraction.
        extract_runes(&block.tree, source_bytes, rune_q, line_offset, &mut symbols);

        // Svelte 4 legacy prop extraction.
        extract_legacy_props(
            &block.tree,
            source_bytes,
            legacy_prop_q,
            line_offset,
            &mut symbols,
        );
    }

    FileSymbols {
        symbols,
        imports,
        exports,
    }
}

/// Run a symbol extraction query against a single tree and collect results.
///
/// `line_offset` adjusts line numbers for Svelte script blocks
/// (which are re-parsed from a sub-range of the original file).
fn extract_from_tree(tree: &Tree, source: &[u8], query: &Query, line_offset: usize) -> FileSymbols {
    let mut symbols = Vec::new();
    let mut imports = Vec::new();
    let mut exports = Vec::new();

    let root = tree.root_node();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, root, source);

    let capture_names = query.capture_names();

    while let Some(m) = matches.next() {
        let mut name_str: Option<String> = None;
        let mut kind: Option<SymbolKind> = None;
        let mut decl_node: Option<Node> = None;
        let mut is_import = false;
        let mut is_dynamic_import = false;
        let mut is_export_stmt = false;
        let mut import_source_str: Option<String> = None;

        for capture in m.captures {
            let capture_name = capture_names[capture.index as usize];
            let node = capture.node;
            let text = node.utf8_text(source).unwrap_or("");

            match capture_name {
                // Function declarations
                "func_name" | "gen_func_name" => {
                    name_str = Some(text.to_string());
                    kind = Some(SymbolKind::Function);
                }
                "function_decl" | "generator_function_decl" => {
                    decl_node = Some(node);
                }

                // Arrow functions
                "arrow_name" | "var_arrow_name" => {
                    name_str = Some(text.to_string());
                    kind = Some(SymbolKind::ArrowFunction);
                }
                "arrow_decl" | "var_arrow_decl" => {
                    decl_node = Some(node);
                }

                // Function expressions assigned to variables
                "func_expr_name" | "var_func_expr_name" => {
                    name_str = Some(text.to_string());
                    kind = Some(SymbolKind::Function);
                }
                "func_expr_decl" | "var_func_expr_decl" => {
                    decl_node = Some(node);
                }

                // Classes
                "class_name" | "abstract_class_name" => {
                    name_str = Some(text.to_string());
                    kind = Some(SymbolKind::Class);
                }
                "class_decl" | "abstract_class_decl" => {
                    decl_node = Some(node);
                }

                // Methods
                "method_name" => {
                    name_str = Some(text.to_string());
                    kind = Some(SymbolKind::Method);
                }
                "method_def" => {
                    decl_node = Some(node);
                }

                // Interfaces
                "interface_name" => {
                    name_str = Some(text.to_string());
                    kind = Some(SymbolKind::Interface);
                }
                "interface_decl" => {
                    decl_node = Some(node);
                }

                // Type aliases
                "type_alias_name" => {
                    name_str = Some(text.to_string());
                    kind = Some(SymbolKind::TypeAlias);
                }
                "type_alias_decl" => {
                    decl_node = Some(node);
                }

                // Enums
                "enum_name" => {
                    name_str = Some(text.to_string());
                    kind = Some(SymbolKind::Enum);
                }
                "enum_decl" => {
                    decl_node = Some(node);
                }

                // Imports
                "import_source" => {
                    is_import = true;
                    import_source_str = Some(strip_quotes(text));
                }
                "import_stmt" => {
                    decl_node = Some(node);
                }

                // Dynamic imports
                "dynamic_import_source" => {
                    is_dynamic_import = true;
                    import_source_str = Some(strip_quotes(text));
                }
                "dynamic_import" => {
                    decl_node = Some(node);
                }

                // Exports
                "export_stmt" => {
                    is_export_stmt = true;
                    decl_node = Some(node);
                }

                _ => {}
            }
        }

        // Process the match into the appropriate output type.
        if is_dynamic_import {
            if let (Some(source_text), Some(node)) = (import_source_str, decl_node) {
                imports.push(Import {
                    source: source_text,
                    specifiers: Vec::new(),
                    kind: ImportKind::Dynamic,
                    line: node.start_position().row + 1 + line_offset,
                });
            }
        } else if is_import {
            if let (Some(source_text), Some(node)) = (import_source_str, decl_node) {
                let (specifiers, import_kind) = parse_import_clause(node, source);
                imports.push(Import {
                    source: source_text,
                    specifiers,
                    kind: import_kind,
                    line: node.start_position().row + 1 + line_offset,
                });
            }
        } else if is_export_stmt {
            if let Some(node) = decl_node {
                let mut parsed_exports = parse_export_statement(node, source, line_offset);
                exports.append(&mut parsed_exports);
            }
        } else if let (Some(name), Some(sym_kind), Some(node)) = (name_str, kind, decl_node) {
            let exported = is_node_exported(node);
            let signature = build_signature(node, &sym_kind, source);
            let start_line = node.start_position().row + 1 + line_offset;
            let end_line = node.end_position().row + 1 + line_offset;

            symbols.push(Symbol {
                name,
                kind: sym_kind,
                line: start_line,
                end_line,
                exported,
                signature,
            });
        }
    }

    FileSymbols {
        symbols,
        imports,
        exports,
    }
}

/// Check if a declaration node's parent is an `export_statement`.
fn is_node_exported(node: Node) -> bool {
    node.parent()
        .is_some_and(|parent| parent.kind() == "export_statement")
}

/// Strip surrounding quotes from a string literal.
fn strip_quotes(s: &str) -> String {
    let trimmed = s.trim();
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

/// Parse the import clause from an import statement node to determine
/// the import kind and specifier list.
fn parse_import_clause(import_node: Node, source: &[u8]) -> (Vec<String>, ImportKind) {
    let text = import_node.utf8_text(source).unwrap_or("");

    // Check for type-only import: `import type { ... } from '...'`
    let is_type_only = text.starts_with("import type ");

    let mut has_clause = false;
    let mut specifiers = Vec::new();
    let mut has_default = false;
    let mut has_namespace = false;
    let mut has_named = false;

    let mut child_cursor = import_node.walk();
    for child in import_node.children(&mut child_cursor) {
        if child.kind() == "import_clause" {
            has_clause = true;
            parse_import_clause_children(
                child,
                source,
                &mut specifiers,
                &mut has_default,
                &mut has_namespace,
                &mut has_named,
            );
        }
    }

    if !has_clause {
        return (Vec::new(), ImportKind::SideEffect);
    }

    if is_type_only {
        return (specifiers, ImportKind::TypeOnly);
    }

    if has_namespace {
        return (specifiers, ImportKind::Namespace);
    }

    if has_default && !has_named {
        return (specifiers, ImportKind::Default);
    }

    (specifiers, ImportKind::Named)
}

/// Walk import clause children to extract specifiers and classify the import.
fn parse_import_clause_children(
    clause_node: Node,
    source: &[u8],
    specifiers: &mut Vec<String>,
    has_default: &mut bool,
    has_namespace: &mut bool,
    has_named: &mut bool,
) {
    let mut cursor = clause_node.walk();
    for child in clause_node.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                *has_default = true;
                if let Ok(name) = child.utf8_text(source) {
                    specifiers.push(name.to_string());
                }
            }
            "namespace_import" => {
                *has_namespace = true;
                if let Ok(text) = child.utf8_text(source) {
                    if let Some(alias) = text.strip_prefix("* as ") {
                        specifiers.push(alias.trim().to_string());
                    } else {
                        specifiers.push(text.to_string());
                    }
                }
            }
            "named_imports" => {
                *has_named = true;
                let mut inner_cursor = child.walk();
                for import_spec in child.children(&mut inner_cursor) {
                    if import_spec.kind() == "import_specifier"
                        && let Ok(text) = import_spec.utf8_text(source)
                    {
                        let name = text
                            .split_once(" as ")
                            .map(|(_, a)| a.trim())
                            .unwrap_or(text.trim());
                        specifiers.push(name.to_string());
                    }
                }
            }
            _ => {}
        }
    }
}

/// Parse an export statement node into one or more Export entries.
fn parse_export_statement(export_node: Node, source: &[u8], line_offset: usize) -> Vec<Export> {
    let mut exports = Vec::new();
    let line = export_node.start_position().row + 1 + line_offset;
    let text = export_node.utf8_text(source).unwrap_or("");

    let is_type_only = text.starts_with("export type ");

    // First pass: collect flags.
    let mut has_default = false;
    let mut has_source = false;
    let mut has_star = false;
    {
        let mut pre_cursor = export_node.walk();
        for child in export_node.children(&mut pre_cursor) {
            match child.kind() {
                "default" => has_default = true,
                "string" => has_source = true,
                "*" => has_star = true,
                _ => {}
            }
        }
    }

    // Second pass: process children.
    let mut child_cursor = export_node.walk();
    for child in export_node.children(&mut child_cursor) {
        match child.kind() {
            "export_clause" => {
                let mut inner_cursor = child.walk();
                for spec in child.children(&mut inner_cursor) {
                    if spec.kind() == "export_specifier"
                        && let Ok(spec_text) = spec.utf8_text(source)
                    {
                        let name = spec_text
                            .split_once(" as ")
                            .map(|(_, a)| a.trim())
                            .unwrap_or(spec_text.trim());
                        let kind = if is_type_only {
                            ExportKind::TypeOnly
                        } else if has_source {
                            ExportKind::ReExport
                        } else if has_default {
                            ExportKind::Default
                        } else {
                            ExportKind::Named
                        };
                        exports.push(Export {
                            name: name.to_string(),
                            kind,
                            line,
                            source: extract_export_source(export_node, source),
                        });
                    }
                }
            }
            // Inline exports: `export function foo()`, `export class Bar`, etc.
            "function_declaration"
            | "generator_function_declaration"
            | "class_declaration"
            | "abstract_class_declaration"
            | "interface_declaration"
            | "type_alias_declaration"
            | "enum_declaration" => {
                if let Some(name) = extract_declaration_name(child, source) {
                    let kind = if is_type_only {
                        ExportKind::TypeOnly
                    } else if has_default {
                        ExportKind::Default
                    } else {
                        ExportKind::Named
                    };
                    exports.push(Export {
                        name,
                        kind,
                        line,
                        source: None,
                    });
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                let mut var_cursor = child.walk();
                for declarator in child.children(&mut var_cursor) {
                    if declarator.kind() == "variable_declarator" {
                        let mut name_cursor = declarator.walk();
                        for name_child in declarator.children(&mut name_cursor) {
                            if name_child.kind() == "identifier" {
                                if let Ok(name) = name_child.utf8_text(source) {
                                    let kind = if is_type_only {
                                        ExportKind::TypeOnly
                                    } else if has_default {
                                        ExportKind::Default
                                    } else {
                                        ExportKind::Named
                                    };
                                    exports.push(Export {
                                        name: name.to_string(),
                                        kind,
                                        line,
                                        source: None,
                                    });
                                }
                                break;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if has_star && has_source {
        exports.push(Export {
            name: "*".to_string(),
            kind: ExportKind::StarReExport,
            line,
            source: extract_export_source(export_node, source),
        });
    }

    if has_default && exports.is_empty() {
        exports.push(Export {
            name: "default".to_string(),
            kind: ExportKind::Default,
            line,
            source: None,
        });
    }

    exports
}

/// Extract the name from a declaration node (function, class, interface, type, enum).
fn extract_declaration_name(node: Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" || child.kind() == "type_identifier" {
            return child.utf8_text(source).ok().map(|s| s.to_string());
        }
    }
    None
}

fn extract_export_source(export_node: Node, source: &[u8]) -> Option<String> {
    let mut cursor = export_node.walk();
    export_node
        .children(&mut cursor)
        .find(|child| child.kind() == "string")
        .and_then(|child| child.utf8_text(source).ok())
        .map(strip_quotes)
}

/// Build a human-readable signature for a symbol.
fn build_signature(node: Node, kind: &SymbolKind, source: &[u8]) -> Option<String> {
    match kind {
        SymbolKind::Function | SymbolKind::ArrowFunction | SymbolKind::Method => {
            build_function_signature(node, source)
        }
        _ => None,
    }
}

/// Build function signature from the declaration or value node.
fn build_function_signature(node: Node, source: &[u8]) -> Option<String> {
    let func_node = find_function_node(node);

    let mut params_text = String::new();
    let mut return_type = String::new();

    let mut cursor = func_node.walk();
    for child in func_node.children(&mut cursor) {
        match child.kind() {
            "formal_parameters" => {
                if let Ok(text) = child.utf8_text(source) {
                    params_text = text.to_string();
                }
            }
            "type_annotation" => {
                if let Ok(text) = child.utf8_text(source) {
                    let stripped = text
                        .strip_prefix(": ")
                        .unwrap_or(text.trim_start_matches(':').trim());
                    return_type = stripped.to_string();
                }
            }
            _ => {}
        }
    }

    if params_text.is_empty() {
        return None;
    }

    if return_type.is_empty() {
        Some(params_text)
    } else {
        Some(format!("{params_text} -> {return_type}"))
    }
}

/// Find the function/arrow node inside a declaration (handles variable wrappers).
fn find_function_node(node: Node) -> Node {
    if node.kind() == "lexical_declaration" || node.kind() == "variable_declaration" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_declarator" {
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "arrow_function" || inner.kind() == "function_expression" {
                        return inner;
                    }
                }
            }
        }
    }
    node
}

/// Extract Svelte 5 runes from a script block.
fn extract_runes(
    tree: &Tree,
    source: &[u8],
    query: &Query,
    line_offset: usize,
    symbols: &mut Vec<Symbol>,
) {
    let root = tree.root_node();
    let mut cursor = QueryCursor::new();
    let capture_names = query.capture_names();

    let mut matches = cursor.matches(query, root, source);
    while let Some(m) = matches.next() {
        let mut rune_name: Option<String> = None;
        let mut rune_node: Option<Node> = None;

        for capture in m.captures {
            let cap_name = capture_names[capture.index as usize];
            match cap_name {
                "rune_name" => {
                    rune_name = capture.node.utf8_text(source).ok().map(str::to_string);
                }
                "rune_call" => {
                    rune_node = Some(capture.node);
                }
                _ => {}
            }
        }

        if let (Some(name), Some(node)) = (rune_name, rune_node) {
            let var_name = find_rune_variable_name(node, source);
            let display_name = if let Some(ref var) = var_name {
                format!("{name}({var})")
            } else {
                name.clone()
            };

            let line = node.start_position().row + 1 + line_offset;
            symbols.push(Symbol {
                name: display_name,
                kind: SymbolKind::Rune,
                line,
                end_line: node.end_position().row + 1 + line_offset,
                exported: false,
                signature: None,
            });
        }
    }
}

/// Walk up from a rune call_expression to find the variable it's assigned to.
fn find_rune_variable_name(node: Node, source: &[u8]) -> Option<String> {
    let parent = node.parent()?;
    if parent.kind() == "variable_declarator" {
        let mut cursor = parent.walk();
        for child in parent.children(&mut cursor) {
            if child.kind() == "identifier" {
                return child.utf8_text(source).ok().map(|s| s.to_string());
            }
        }
    }
    None
}

/// Extract Svelte 4 legacy `export let` props from a script block.
fn extract_legacy_props(
    tree: &Tree,
    source: &[u8],
    query: &Query,
    line_offset: usize,
    symbols: &mut Vec<Symbol>,
) {
    let root = tree.root_node();
    let mut cursor = QueryCursor::new();
    let capture_names = query.capture_names();

    let mut matches = cursor.matches(query, root, source);
    while let Some(m) = matches.next() {
        let mut prop_name: Option<String> = None;
        let mut prop_node: Option<Node> = None;

        for capture in m.captures {
            let cap_name = capture_names[capture.index as usize];
            match cap_name {
                "legacy_prop_name" => {
                    prop_name = capture.node.utf8_text(source).ok().map(str::to_string);
                }
                "legacy_prop" => {
                    prop_node = Some(capture.node);
                }
                _ => {}
            }
        }

        if let (Some(name), Some(node)) = (prop_name, prop_node) {
            let line = node.start_position().row + 1 + line_offset;
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Variable,
                line,
                end_line: node.end_position().row + 1 + line_offset,
                exported: true,
                signature: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::detect_language;
    use crate::output::SourceFile;
    use crate::parse::parse_files_sequential;
    use std::fs;
    use std::path::Path;

    /// Helper: write a fixture file, parse it, and extract symbols.
    fn extract_from_source(filename: &str, source: &str) -> FileSymbols {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(filename);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, source).unwrap();

        let file_path = Path::new(filename);
        let sf = SourceFile {
            path: filename.to_string(),
            language: detect_language(file_path).unwrap(),
            declaration_only: crate::language::is_declaration_file(file_path),
        };

        let result = parse_files_sequential(&[sf], dir.path());
        assert!(
            result.warnings.is_empty(),
            "parse warnings: {:?}",
            result.warnings
        );
        assert_eq!(result.files.len(), 1);

        extract_symbols(&result.files[0])
    }

    // --- Function declarations ---

    #[test]
    fn extracts_named_function_declaration() {
        let syms = extract_from_source(
            "main.ts",
            "function greet(name: string): string {\n  return `Hello ${name}`;\n}\n",
        );

        assert_eq!(syms.symbols.len(), 1);
        let func = &syms.symbols[0];
        assert_eq!(func.name, "greet");
        assert_eq!(func.kind, SymbolKind::Function);
        assert_eq!(func.line, 1);
        assert!(!func.exported);
    }

    #[test]
    fn extracts_exported_function() {
        let syms = extract_from_source(
            "utils.ts",
            "export function add(a: number, b: number): number {\n  return a + b;\n}\n",
        );

        let funcs: Vec<_> = syms
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();
        assert_eq!(funcs.len(), 1);
        assert!(funcs[0].exported);
        assert_eq!(funcs[0].name, "add");

        assert_eq!(syms.exports.len(), 1);
        assert_eq!(syms.exports[0].name, "add");
        assert_eq!(syms.exports[0].kind, ExportKind::Named);
    }

    #[test]
    fn extracts_arrow_function() {
        let syms =
            extract_from_source("arrow.ts", "const double = (n: number): number => n * 2;\n");

        assert_eq!(syms.symbols.len(), 1);
        assert_eq!(syms.symbols[0].name, "double");
        assert_eq!(syms.symbols[0].kind, SymbolKind::ArrowFunction);
    }

    #[test]
    fn extracts_async_function() {
        let syms = extract_from_source(
            "async.ts",
            "async function fetchData(): Promise<void> {\n  await fetch('/api');\n}\n",
        );

        assert_eq!(syms.symbols.len(), 1);
        assert_eq!(syms.symbols[0].name, "fetchData");
        assert_eq!(syms.symbols[0].kind, SymbolKind::Function);
    }

    // --- Classes ---

    #[test]
    fn extracts_class_with_methods() {
        let syms = extract_from_source(
            "class.ts",
            "class Greeter {\n  greet(name: string): string {\n    return `Hello ${name}`;\n  }\n}\n",
        );

        let classes: Vec<_> = syms
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name, "Greeter");

        let methods: Vec<_> = syms
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].name, "greet");
    }

    #[test]
    fn extracts_abstract_class() {
        let syms = extract_from_source(
            "abstract.ts",
            "abstract class Shape {\n  abstract area(): number;\n}\n",
        );

        let classes: Vec<_> = syms
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name, "Shape");
    }

    // --- Interfaces and types ---

    #[test]
    fn extracts_interface() {
        let syms = extract_from_source(
            "types.ts",
            "interface User {\n  name: string;\n  age: number;\n}\n",
        );

        assert_eq!(syms.symbols.len(), 1);
        assert_eq!(syms.symbols[0].name, "User");
        assert_eq!(syms.symbols[0].kind, SymbolKind::Interface);
    }

    #[test]
    fn extracts_type_alias() {
        let syms = extract_from_source("types.ts", "type ID = string | number;\n");

        assert_eq!(syms.symbols.len(), 1);
        assert_eq!(syms.symbols[0].name, "ID");
        assert_eq!(syms.symbols[0].kind, SymbolKind::TypeAlias);
    }

    #[test]
    fn extracts_enum() {
        let syms = extract_from_source("status.ts", "enum Status {\n  Active,\n  Inactive,\n}\n");

        assert_eq!(syms.symbols.len(), 1);
        assert_eq!(syms.symbols[0].name, "Status");
        assert_eq!(syms.symbols[0].kind, SymbolKind::Enum);
    }

    // --- Imports ---

    #[test]
    fn extracts_named_import() {
        let syms = extract_from_source(
            "imports.ts",
            "import { useState, useEffect } from 'react';\n",
        );

        assert_eq!(syms.imports.len(), 1);
        assert_eq!(syms.imports[0].source, "react");
        assert_eq!(syms.imports[0].kind, ImportKind::Named);
        assert!(syms.imports[0].specifiers.contains(&"useState".to_string()));
        assert!(
            syms.imports[0]
                .specifiers
                .contains(&"useEffect".to_string())
        );
    }

    #[test]
    fn extracts_default_import() {
        let syms = extract_from_source("imports.ts", "import React from 'react';\n");

        assert_eq!(syms.imports.len(), 1);
        assert_eq!(syms.imports[0].source, "react");
        assert_eq!(syms.imports[0].kind, ImportKind::Default);
        assert_eq!(syms.imports[0].specifiers, vec!["React"]);
    }

    #[test]
    fn extracts_namespace_import() {
        let syms = extract_from_source("imports.ts", "import * as path from 'path';\n");

        assert_eq!(syms.imports.len(), 1);
        assert_eq!(syms.imports[0].source, "path");
        assert_eq!(syms.imports[0].kind, ImportKind::Namespace);
        assert_eq!(syms.imports[0].specifiers, vec!["path"]);
    }

    #[test]
    fn extracts_side_effect_import() {
        let syms = extract_from_source("imports.ts", "import './styles.css';\n");

        assert_eq!(syms.imports.len(), 1);
        assert_eq!(syms.imports[0].source, "./styles.css");
        assert_eq!(syms.imports[0].kind, ImportKind::SideEffect);
        assert!(syms.imports[0].specifiers.is_empty());
    }

    #[test]
    fn extracts_type_only_import() {
        let syms = extract_from_source("imports.ts", "import type { User } from './types';\n");

        assert_eq!(syms.imports.len(), 1);
        assert_eq!(syms.imports[0].source, "./types");
        assert_eq!(syms.imports[0].kind, ImportKind::TypeOnly);
    }

    #[test]
    fn extracts_dynamic_import() {
        let syms =
            extract_from_source("dynamic.ts", "const mod = await import('./lazy-module');\n");

        assert_eq!(syms.imports.len(), 1);
        assert_eq!(syms.imports[0].source, "./lazy-module");
        assert_eq!(syms.imports[0].kind, ImportKind::Dynamic);
    }

    // --- Exports ---

    #[test]
    fn extracts_default_export() {
        let syms = extract_from_source("default.ts", "const value = 42;\nexport default value;\n");

        let default_exports: Vec<_> = syms
            .exports
            .iter()
            .filter(|e| e.kind == ExportKind::Default)
            .collect();
        assert_eq!(default_exports.len(), 1);
        assert_eq!(default_exports[0].name, "default");
    }

    #[test]
    fn extracts_default_exported_function_declaration() {
        let syms = extract_from_source("default.ts", "export default function App() {}\n");

        let default_exports: Vec<_> = syms
            .exports
            .iter()
            .filter(|e| e.kind == ExportKind::Default)
            .collect();
        assert_eq!(default_exports.len(), 1);
        assert_eq!(default_exports[0].name, "App");
    }

    #[test]
    fn extracts_named_export_clause() {
        let syms =
            extract_from_source("named.ts", "const a = 1;\nconst b = 2;\nexport { a, b };\n");

        assert_eq!(syms.exports.len(), 2);
        let names: Vec<_> = syms.exports.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
    }

    #[test]
    fn extracts_re_export() {
        let syms = extract_from_source("barrel.ts", "export { foo, bar } from './utils';\n");

        assert_eq!(syms.exports.len(), 2);
        assert!(syms.exports.iter().all(|e| e.kind == ExportKind::ReExport));
    }

    #[test]
    fn extracts_star_re_export() {
        let syms = extract_from_source("barrel.ts", "export * from './utils';\n");

        assert_eq!(syms.exports.len(), 1);
        assert_eq!(syms.exports[0].kind, ExportKind::StarReExport);
        assert_eq!(syms.exports[0].name, "*");
    }

    #[test]
    fn extracts_type_only_export() {
        let syms = extract_from_source("types.ts", "export type { User, Admin } from './types';\n");

        assert!(syms.exports.iter().all(|e| e.kind == ExportKind::TypeOnly));
    }

    // --- JavaScript subset ---

    #[test]
    fn javascript_extracts_functions_but_not_interfaces() {
        let syms = extract_from_source(
            "utils.js",
            "function greet(name) {\n  return `Hello ${name}`;\n}\n",
        );

        assert_eq!(syms.symbols.len(), 1);
        assert_eq!(syms.symbols[0].name, "greet");
        assert_eq!(syms.symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn javascript_extracts_class() {
        let syms = extract_from_source("app.js", "class App {\n  render() { return null; }\n}\n");

        let classes: Vec<_> = syms
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name, "App");
    }

    // --- Svelte ---

    #[test]
    fn svelte_extracts_component_symbol() {
        let syms = extract_from_source(
            "Button.svelte",
            "<script>\n  let label = 'Click me';\n</script>\n<button>{label}</button>\n",
        );

        let components: Vec<_> = syms
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Component)
            .collect();
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].name, "Button");
        assert!(components[0].exported);
    }

    #[test]
    fn svelte_extracts_functions_from_script() {
        let syms = extract_from_source(
            "Counter.svelte",
            "<script>\n  function increment() { count += 1; }\n</script>\n",
        );

        let funcs: Vec<_> = syms
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "increment");
    }

    #[test]
    fn svelte_extracts_imports() {
        let syms = extract_from_source(
            "Page.svelte",
            "<script>\n  import { onMount } from 'svelte';\n</script>\n",
        );

        assert_eq!(syms.imports.len(), 1);
        assert_eq!(syms.imports[0].source, "svelte");
    }

    #[test]
    fn svelte_extracts_runes() {
        let syms = extract_from_source(
            "Counter.svelte",
            "<script>\n  let count = $state(0);\n  let doubled = $derived(count * 2);\n</script>\n",
        );

        let runes: Vec<_> = syms
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Rune)
            .collect();
        assert_eq!(runes.len(), 2, "runes found: {:?}", runes);
        assert!(runes.iter().any(|r| r.name.contains("$state")));
        assert!(runes.iter().any(|r| r.name.contains("$derived")));
    }

    #[test]
    fn svelte_extracts_legacy_props() {
        let syms = extract_from_source(
            "Card.svelte",
            "<script>\n  export let title;\n  export let description;\n</script>\n",
        );

        let props: Vec<_> = syms
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable && s.exported)
            .collect();
        assert_eq!(props.len(), 2);
        let names: Vec<_> = props.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"title"));
        assert!(names.contains(&"description"));
    }

    // --- Edge cases ---

    #[test]
    fn empty_file_produces_no_symbols() {
        let syms = extract_from_source("empty.ts", "");
        assert!(syms.symbols.is_empty());
        assert!(syms.imports.is_empty());
        assert!(syms.exports.is_empty());
    }

    #[test]
    fn file_with_only_comments() {
        let syms =
            extract_from_source("comments.ts", "// This is a comment\n/* Block comment */\n");
        assert!(syms.symbols.is_empty());
    }

    #[test]
    fn mixed_typescript_and_tsx_files_both_extract_symbols() {
        let dir = tempfile::tempdir().unwrap();

        fs::write(dir.path().join("alpha.ts"), "export function alpha() {}\n").unwrap();
        fs::write(
            dir.path().join("App.tsx"),
            "export default function App() { return <div />; }\n",
        )
        .unwrap();

        let source_files = vec![
            SourceFile {
                path: "App.tsx".to_string(),
                language: detect_language(Path::new("App.tsx")).unwrap(),
                declaration_only: false,
            },
            SourceFile {
                path: "alpha.ts".to_string(),
                language: detect_language(Path::new("alpha.ts")).unwrap(),
                declaration_only: false,
            },
        ];

        let parsed = parse_files_sequential(&source_files, dir.path());
        assert!(
            parsed.warnings.is_empty(),
            "parse warnings: {:?}",
            parsed.warnings
        );

        let extracted: Vec<_> = parsed.files.iter().map(extract_symbols).collect();
        let all_names: Vec<_> = extracted
            .iter()
            .flat_map(|file| file.symbols.iter().map(|symbol| symbol.name.as_str()))
            .collect();

        assert!(
            all_names.contains(&"App"),
            "missing TSX symbols: {all_names:?}"
        );
        assert!(
            all_names.contains(&"alpha"),
            "missing TypeScript symbols: {all_names:?}"
        );
    }

    #[test]
    fn malformed_file_extracts_partial_symbols() {
        let syms = extract_from_source(
            "bad.ts",
            "function valid() { return 1; }\nfunction { broken }\n",
        );

        assert!(
            !syms.symbols.is_empty(),
            "should extract symbols from malformed file"
        );
    }

    #[test]
    fn function_signature_includes_params_and_return_type() {
        let syms = extract_from_source(
            "sig.ts",
            "function add(a: number, b: number): number {\n  return a + b;\n}\n",
        );

        assert_eq!(syms.symbols.len(), 1);
        let sig = syms.symbols[0].signature.as_deref().unwrap();
        assert!(sig.contains("a: number"), "signature: {sig}");
        assert!(sig.contains("number"), "signature: {sig}");
    }

    #[test]
    fn svelte_without_script_has_only_component() {
        let syms = extract_from_source("Static.svelte", "<h1>Hello world</h1>\n");

        assert_eq!(syms.symbols.len(), 1);
        assert_eq!(syms.symbols[0].kind, SymbolKind::Component);
        assert_eq!(syms.symbols[0].name, "Static");
    }

    #[test]
    fn multiple_symbols_in_one_file() {
        let syms = extract_from_source(
            "multi.ts",
            r#"
interface Config { key: string; }
type ID = string;
enum Status { Active, Inactive }
class App {}
function run(): void {}
const helper = () => {};
"#,
        );

        let kinds: Vec<_> = syms.symbols.iter().map(|s| s.kind.clone()).collect();
        assert!(kinds.contains(&SymbolKind::Interface));
        assert!(kinds.contains(&SymbolKind::TypeAlias));
        assert!(kinds.contains(&SymbolKind::Enum));
        assert!(kinds.contains(&SymbolKind::Class));
        assert!(kinds.contains(&SymbolKind::Function));
        assert!(kinds.contains(&SymbolKind::ArrowFunction));
    }
}
