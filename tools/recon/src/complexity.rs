use tree_sitter::{Node, Tree};

use crate::output::FunctionMetrics;

/// Line count breakdown for a source file or function.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LocCounts {
    pub total_lines: usize,
    pub code_lines: usize,
    pub comment_lines: usize,
    pub blank_lines: usize,
}

/// Tree-sitter node kinds that represent decision points for cyclomatic complexity.
const BRANCH_NODES: &[&str] = &[
    "if_statement",
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
    "catch_clause",
    "ternary_expression",
    "switch_case", // not switch_default
];

/// Logical operators that add branches: `&&`, `||`, `??`.
const BRANCH_OPERATORS: &[&str] = &["&&", "||", "??"];

/// Compute cyclomatic complexity for a single function/method AST node.
///
/// Base complexity is 1. Each decision point adds 1.
pub fn compute_cyclomatic_complexity(node: &Node, source: &[u8]) -> u32 {
    let mut complexity: u32 = 1;
    complexity = complexity.saturating_add(count_branches(node, source));
    complexity
}

/// Count lines of code, comments, and blanks in a source file.
///
/// Uses tree-sitter to identify comment nodes. A line with both code and
/// a comment is counted as a code line (not double-counted).
pub fn count_lines(source: &str, tree: &Tree) -> LocCounts {
    if source.is_empty() {
        return LocCounts::default();
    }

    let lines: Vec<&str> = source.lines().collect();
    let total_lines = lines.len();

    // Build a set of line numbers that contain comment nodes.
    let mut comment_line_set = vec![false; total_lines];
    collect_comment_lines(&tree.root_node(), &mut comment_line_set);

    // Build a set of line numbers that contain non-comment, non-whitespace code.
    let mut code_line_set = vec![false; total_lines];
    collect_code_lines(&tree.root_node(), &mut code_line_set);

    let mut blank_lines = 0;
    let mut code_lines = 0;
    let mut comment_lines = 0;

    for (i, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            blank_lines += 1;
        } else if code_line_set[i] {
            code_lines += 1;
        } else if comment_line_set[i] {
            comment_lines += 1;
        } else {
            // Fallback: lines with content not captured by tree-sitter (e.g. error recovery).
            code_lines += 1;
        }
    }

    LocCounts {
        total_lines,
        code_lines,
        comment_lines,
        blank_lines,
    }
}

/// Mark lines that contain comment nodes.
fn collect_comment_lines(node: &Node, lines: &mut [bool]) {
    if node.kind() == "comment" {
        let start = node.start_position().row;
        let end = node.end_position().row;
        for line in start..=end.min(lines.len().saturating_sub(1)) {
            lines[line] = true;
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_comment_lines(&child, lines);
    }
}

/// Mark lines that contain non-comment code nodes.
fn collect_code_lines(node: &Node, code_lines: &mut [bool]) {
    if node.kind() == "comment" {
        return;
    }
    if node.child_count() == 0 && !node.kind().is_empty() {
        let start = node.start_position().row;
        let end = node.end_position().row;
        for line in start..=end.min(code_lines.len().saturating_sub(1)) {
            code_lines[line] = true;
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_code_lines(&child, code_lines);
    }
}

/// Node kinds that represent function-like constructs for metric extraction.
const FUNCTION_NODE_KINDS: &[&str] = &[
    "function_declaration",
    "generator_function_declaration",
    "method_definition",
    "arrow_function",
    "function_expression",
    "generator_function",
];

/// Extract per-function metrics from a parse tree.
///
/// Walks the AST to find all function-like nodes, computes cyclomatic
/// complexity, nesting depth, LOC, and parameter count for each.
pub fn extract_function_metrics(tree: &Tree, source: &str) -> Vec<FunctionMetrics> {
    let mut results = Vec::new();
    let root = tree.root_node();
    collect_functions(&root, source, &mut results);
    results
}

fn collect_functions(node: &Node, source: &str, results: &mut Vec<FunctionMetrics>) {
    if FUNCTION_NODE_KINDS.contains(&node.kind())
        && let Some(m) = build_function_metrics(node, source)
    {
        results.push(m);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_functions(&child, source, results);
    }
}

fn build_function_metrics(node: &Node, source: &str) -> Option<FunctionMetrics> {
    let name = extract_function_name(node, source)?;
    let src_bytes = source.as_bytes();

    let start_line = node.start_position().row + 1; // 1-indexed
    let end_line = node.end_position().row + 1;
    let lines_of_code = end_line.saturating_sub(start_line) + 1;

    let complexity = compute_cyclomatic_complexity(node, src_bytes);
    let nesting = compute_max_nesting_depth(node);
    let params = count_parameters(node);

    Some(FunctionMetrics {
        name,
        line: start_line,
        end_line,
        lines_of_code,
        cyclomatic_complexity: complexity,
        max_nesting_depth: nesting,
        parameter_count: params,
    })
}

/// Extract the name of a function-like node.
///
/// For arrow functions assigned to variables (`const foo = () => ...`),
/// the name comes from the variable declaration parent.
fn extract_function_name(node: &Node, source: &str) -> Option<String> {
    // Named functions and methods have a "name" field.
    if let Some(name_node) = node.child_by_field_name("name") {
        return Some(name_node.utf8_text(source.as_bytes()).ok()?.to_string());
    }

    // Arrow functions: walk up to find variable declarator or assignment.
    if matches!(node.kind(), "arrow_function" | "function_expression") {
        let parent = node.parent()?;
        if parent.kind() == "variable_declarator"
            && let Some(name_node) = parent.child_by_field_name("name")
        {
            return Some(name_node.utf8_text(source.as_bytes()).ok()?.to_string());
        }
        if parent.kind() == "assignment_expression"
            && let Some(left) = parent.child_by_field_name("left")
        {
            return Some(left.utf8_text(source.as_bytes()).ok()?.to_string());
        }
    }

    // Anonymous functions without a name — skip them for metrics.
    None
}

/// Count formal parameters of a function-like node.
fn count_parameters(node: &Node) -> u32 {
    let Some(params) = node.child_by_field_name("parameters") else {
        return 0;
    };
    let mut count: u32 = 0;
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        if child.is_named() {
            count += 1;
        }
    }
    count
}

/// Node kinds that increase nesting depth.
const NESTING_NODES: &[&str] = &[
    "if_statement",
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
    "switch_statement",
    "try_statement",
    "catch_clause",
];

/// Compute the maximum nesting depth within a function/method AST node.
///
/// Nesting depth 0 means no control structures. Each nesting-incrementing
/// node adds 1 to the depth of its children.
pub fn compute_max_nesting_depth(node: &Node) -> u32 {
    fn walk(node: &Node, current_depth: u32) -> u32 {
        let mut max = current_depth;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            // Don't recurse into nested functions.
            if FUNCTION_NODE_KINDS.contains(&child.kind()) {
                continue;
            }
            let child_depth = if NESTING_NODES.contains(&child.kind()) {
                current_depth + 1
            } else {
                current_depth
            };
            let descendant_max = walk(&child, child_depth);
            if descendant_max > max {
                max = descendant_max;
            }
        }

        max
    }

    // Start at depth 0 inside the function body.
    walk(node, 0)
}

/// Recursively count branch points within a node.
///
/// Stops recursing at nested function boundaries so each function's
/// complexity is measured independently.
fn count_branches(node: &Node, source: &[u8]) -> u32 {
    let mut count: u32 = 0;

    if BRANCH_NODES.contains(&node.kind()) {
        count = count.saturating_add(1);
    }

    // Logical operators in binary expressions.
    if node.kind() == "binary_expression"
        && let Some(op_node) = node.child_by_field_name("operator")
        && BRANCH_OPERATORS.contains(&op_node.utf8_text(source).unwrap_or(""))
    {
        count = count.saturating_add(1);
    }

    // Optional chaining `?.` on member expressions and call expressions.
    if (node.kind() == "member_expression" || node.kind() == "call_expression")
        && node.child_by_field_name("optional_chain").is_some()
    {
        count = count.saturating_add(1);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // Don't recurse into nested function bodies — each function is measured independently.
        if FUNCTION_NODE_KINDS.contains(&child.kind()) {
            continue;
        }
        count = count.saturating_add(count_branches(&child, source));
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{language_tsx, language_typescript};
    use tree_sitter::Parser;

    fn parse_ts(source: &str) -> Tree {
        let mut parser = Parser::new();
        parser.set_language(&language_typescript()).unwrap();
        parser.parse(source, None).unwrap()
    }

    fn parse_tsx(source: &str) -> Tree {
        let mut parser = Parser::new();
        parser.set_language(&language_tsx()).unwrap();
        parser.parse(source, None).unwrap()
    }

    #[test]
    fn function_with_branches_has_correct_complexity() {
        let source = r#"
function classify(x: number): string {
    if (x > 0) {
        if (x > 100) {
            return "big";
        }
        return "positive";
    } else if (x < 0) {
        return "negative";
    }
    for (let i = 0; i < x; i++) {
        console.log(i);
    }
    return x === 0 ? "zero" : "other";
}
"#;
        let tree = parse_ts(source);
        let root = tree.root_node();
        let func = root.named_child(0).unwrap();
        assert_eq!(func.kind(), "function_declaration");
        // 1 base + 2 if + 1 else_if(if_statement) + 1 for + 1 ternary = 6
        let complexity = compute_cyclomatic_complexity(&func, source.as_bytes());
        assert_eq!(complexity, 6);
    }

    #[test]
    fn logical_operators_add_branches() {
        let source = "function check(a: boolean, b: boolean) { return a && b || !a; }";
        let tree = parse_ts(source);
        let root = tree.root_node();
        let func = root.named_child(0).unwrap();
        // 1 base + 1 (&&) + 1 (||) = 3
        let complexity = compute_cyclomatic_complexity(&func, source.as_bytes());
        assert_eq!(complexity, 3);
    }

    #[test]
    fn nullish_coalescing_adds_branch() {
        let source = "function safe(x?: string) { return x ?? 'default'; }";
        let tree = parse_ts(source);
        let root = tree.root_node();
        let func = root.named_child(0).unwrap();
        // 1 base + 1 (??) = 2
        let complexity = compute_cyclomatic_complexity(&func, source.as_bytes());
        assert_eq!(complexity, 2);
    }

    #[test]
    fn switch_case_adds_per_case() {
        let source = r#"
function classify(status: number): string {
    switch (status) {
        case 200: return "ok";
        case 404: return "not found";
        case 500: return "error";
        default: return "unknown";
    }
}
"#;
        let tree = parse_ts(source);
        let root = tree.root_node();
        let func = root.named_child(0).unwrap();
        // 1 base + 3 switch_case (not default) = 4
        let complexity = compute_cyclomatic_complexity(&func, source.as_bytes());
        assert_eq!(complexity, 4);
    }

    #[test]
    fn catch_clause_adds_branch() {
        let source = r#"
function risky() {
    try {
        doSomething();
    } catch (e) {
        handleError(e);
    }
}
"#;
        let tree = parse_ts(source);
        let root = tree.root_node();
        let func = root.named_child(0).unwrap();
        // 1 base + 1 catch = 2
        let complexity = compute_cyclomatic_complexity(&func, source.as_bytes());
        assert_eq!(complexity, 2);
    }

    #[test]
    fn empty_function_has_complexity_one() {
        let source = "function empty() {}";
        let tree = parse_ts(source);
        let root = tree.root_node();
        let func = root.named_child(0).unwrap();
        assert_eq!(compute_cyclomatic_complexity(&func, source.as_bytes()), 1);
    }

    #[test]
    fn simple_function_has_complexity_one() {
        let source = "function greet(name: string) { return `Hello ${name}`; }";
        let tree = parse_ts(source);
        let root = tree.root_node();
        let func = root.child(0).unwrap();
        assert_eq!(func.kind(), "function_declaration");
        let complexity = compute_cyclomatic_complexity(&func, source.as_bytes());
        assert_eq!(complexity, 1);
    }

    // --- Nesting depth tests ---

    #[test]
    fn flat_function_has_nesting_zero() {
        let source = "function flat() { return 42; }";
        let tree = parse_ts(source);
        let func = tree.root_node().named_child(0).unwrap();
        assert_eq!(compute_max_nesting_depth(&func), 0);
    }

    #[test]
    fn single_if_has_nesting_one() {
        let source = r#"
function check(x: number) {
    if (x > 0) {
        return x;
    }
}
"#;
        let tree = parse_ts(source);
        let func = tree.root_node().named_child(0).unwrap();
        assert_eq!(compute_max_nesting_depth(&func), 1);
    }

    #[test]
    fn nested_control_flow_tracks_max_depth() {
        let source = r#"
function deep(items: number[]) {
    for (let item of items) {
        if (item > 0) {
            while (item > 10) {
                item -= 1;
            }
        }
    }
}
"#;
        let tree = parse_ts(source);
        let func = tree.root_node().named_child(0).unwrap();
        // for_in > if > while = 3
        assert_eq!(compute_max_nesting_depth(&func), 3);
    }

    // --- LOC counting tests ---

    #[test]
    fn count_lines_mixed_content() {
        let source =
            "// comment\nconst x = 1;\n\nconst y = 2; // inline comment\n/* block\n   comment */\n";
        let tree = parse_ts(source);
        let counts = count_lines(source, &tree);
        // Lines: "// comment", "const x = 1;", "", "const y = 2; // inline comment", "/* block", "   comment */"
        assert_eq!(counts.total_lines, 6);
        assert_eq!(counts.blank_lines, 1);
        assert_eq!(counts.comment_lines, 3); // line 1 (// comment), line 5 (/* block), line 6 (comment */)
        assert_eq!(counts.code_lines, 2); // line 2, line 4 (has code + comment -> counted as code)
    }

    #[test]
    fn count_lines_empty_source() {
        let source = "";
        let tree = parse_ts(source);
        let counts = count_lines(source, &tree);
        assert_eq!(counts, LocCounts::default());
    }

    #[test]
    fn count_lines_only_comments() {
        let source = "// line 1\n// line 2\n";
        let tree = parse_ts(source);
        let counts = count_lines(source, &tree);
        assert_eq!(counts.total_lines, 2);
        assert_eq!(counts.code_lines, 0);
        assert_eq!(counts.comment_lines, 2);
        assert_eq!(counts.blank_lines, 0);
    }

    #[test]
    fn try_catch_contributes_to_nesting() {
        let source = r#"
function handle() {
    try {
        if (true) {
            doSomething();
        }
    } catch (e) {
        log(e);
    }
}
"#;
        let tree = parse_ts(source);
        let func = tree.root_node().named_child(0).unwrap();
        // try > if = 2
        assert_eq!(compute_max_nesting_depth(&func), 2);
    }

    // --- Per-function metrics extraction tests ---

    #[test]
    fn extract_metrics_for_multiple_functions() {
        let source = r#"
function simple() {
    return 1;
}

function complex(x: number, y: number) {
    if (x > 0) {
        for (let i = 0; i < y; i++) {
            console.log(i);
        }
    }
    return x + y;
}
"#;
        let tree = parse_ts(source);
        let funcs = extract_function_metrics(&tree, source);
        assert_eq!(funcs.len(), 2);

        assert_eq!(funcs[0].name, "simple");
        assert_eq!(funcs[0].cyclomatic_complexity, 1);
        assert_eq!(funcs[0].parameter_count, 0);

        assert_eq!(funcs[1].name, "complex");
        assert_eq!(funcs[1].parameter_count, 2);
        // 1 base + 1 if + 1 for = 3
        assert_eq!(funcs[1].cyclomatic_complexity, 3);
        // if > for = 2
        assert_eq!(funcs[1].max_nesting_depth, 2);
    }

    #[test]
    fn extract_metrics_for_arrow_function() {
        let source = "const add = (a: number, b: number) => a + b;\n";
        let tree = parse_ts(source);
        let funcs = extract_function_metrics(&tree, source);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "add");
        assert_eq!(funcs[0].parameter_count, 2);
        assert_eq!(funcs[0].cyclomatic_complexity, 1);
    }

    #[test]
    fn extract_metrics_for_class_methods() {
        let source = r#"
class Calculator {
    add(a: number, b: number): number {
        return a + b;
    }

    divide(a: number, b: number): number {
        if (b === 0) {
            throw new Error("division by zero");
        }
        return a / b;
    }
}
"#;
        let tree = parse_ts(source);
        let funcs = extract_function_metrics(&tree, source);
        assert_eq!(funcs.len(), 2);

        assert_eq!(funcs[0].name, "add");
        assert_eq!(funcs[0].cyclomatic_complexity, 1);

        assert_eq!(funcs[1].name, "divide");
        assert_eq!(funcs[1].cyclomatic_complexity, 2); // 1 base + 1 if
    }

    #[test]
    fn nested_function_complexity_is_independent() {
        let source = r#"
function outer(x: number) {
    const inner = (y: number) => {
        if (y > 0) { return y; }
        return -y;
    };
    return inner(x);
}
"#;
        let tree = parse_ts(source);
        let funcs = extract_function_metrics(&tree, source);
        assert_eq!(funcs.len(), 2);
        assert_eq!(funcs[0].name, "outer");
        assert_eq!(funcs[0].cyclomatic_complexity, 1); // no branches of its own
        assert_eq!(funcs[1].name, "inner");
        assert_eq!(funcs[1].cyclomatic_complexity, 2); // 1 base + 1 if
    }

    #[test]
    fn extract_metrics_empty_source() {
        let source = "";
        let tree = parse_ts(source);
        let funcs = extract_function_metrics(&tree, source);
        assert!(funcs.is_empty());
    }

    // --- Edge case tests ---

    #[test]
    fn for_in_statement_adds_complexity() {
        let source = r#"
function process(items: string[]) {
    for (const item of items) {
        console.log(item);
    }
}
"#;
        let tree = parse_ts(source);
        let func = tree.root_node().named_child(0).unwrap();
        // 1 base + 1 for_in = 2
        assert_eq!(compute_cyclomatic_complexity(&func, source.as_bytes()), 2);
    }

    #[test]
    fn do_while_adds_complexity_and_nesting() {
        let source = r#"
function countdown(n: number) {
    do {
        n--;
    } while (n > 0);
}
"#;
        let tree = parse_ts(source);
        let func = tree.root_node().named_child(0).unwrap();
        assert_eq!(compute_cyclomatic_complexity(&func, source.as_bytes()), 2); // 1 base + 1 do
        assert_eq!(compute_max_nesting_depth(&func), 1);
    }

    #[test]
    fn deeply_nested_code_reports_correct_depth() {
        let source = r#"
function deep() {
    if (true) {
        for (let i = 0; i < 10; i++) {
            while (true) {
                try {
                    if (i > 5) {
                        break;
                    }
                } catch (e) {
                    // nothing
                }
            }
        }
    }
}
"#;
        let tree = parse_ts(source);
        let func = tree.root_node().named_child(0).unwrap();
        // if > for > while > try > if = 5
        assert_eq!(compute_max_nesting_depth(&func), 5);
    }

    #[test]
    fn javascript_function_metrics_via_tsx_grammar() {
        // JS files are parsed with the TSX grammar — verify it works.
        let source = "function add(a, b) { return a + b; }\n";
        let tree = parse_tsx(source);
        let funcs = extract_function_metrics(&tree, source);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "add");
        assert_eq!(funcs[0].parameter_count, 2);
        assert_eq!(funcs[0].cyclomatic_complexity, 1);
    }

    #[test]
    fn function_expression_in_variable() {
        let source = "const handler = function processEvent(event: Event) { if (event) { handle(event); } };\n";
        let tree = parse_ts(source);
        let funcs = extract_function_metrics(&tree, source);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "processEvent");
        assert_eq!(funcs[0].cyclomatic_complexity, 2); // 1 base + 1 if
    }

    #[test]
    fn loc_counts_crlf_line_endings() {
        let source = "const x = 1;\r\nconst y = 2;\r\n// comment\r\n";
        let tree = parse_ts(source);
        let counts = count_lines(source, &tree);
        assert_eq!(counts.total_lines, 3);
        assert_eq!(counts.code_lines, 2);
        assert_eq!(counts.comment_lines, 1);
    }

    #[test]
    fn loc_counts_only_blank_lines() {
        let source = "\n\n\n";
        let tree = parse_ts(source);
        let counts = count_lines(source, &tree);
        assert_eq!(counts.total_lines, 3);
        assert_eq!(counts.blank_lines, 3);
        assert_eq!(counts.code_lines, 0);
        assert_eq!(counts.comment_lines, 0);
    }

    #[test]
    fn async_generator_function_metrics() {
        let source = r#"
async function* streamData(url: string, maxPages: number) {
    for (let page = 0; page < maxPages; page++) {
        yield await fetch(url);
    }
}
"#;
        let tree = parse_ts(source);
        let funcs = extract_function_metrics(&tree, source);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "streamData");
        assert_eq!(funcs[0].parameter_count, 2);
        // 1 base + 1 for = 2
        assert_eq!(funcs[0].cyclomatic_complexity, 2);
    }

    #[test]
    fn malformed_code_produces_metrics_without_panic() {
        let source = "function broken( { if if if }";
        let tree = parse_ts(source);
        // Should not panic — just produce whatever metrics are extractable.
        let funcs = extract_function_metrics(&tree, source);
        // Malformed code may or may not produce function nodes; just don't crash.
        let _ = funcs;
    }

    #[test]
    fn optional_chaining_member_access_adds_complexity() {
        let source = "function get(obj: any) { return obj?.value; }";
        let tree = parse_ts(source);
        let func = tree.root_node().named_child(0).unwrap();
        // 1 base + 1 optional chain
        assert_eq!(compute_cyclomatic_complexity(&func, source.as_bytes()), 2);
    }

    #[test]
    fn optional_chaining_call_is_not_surfaced_in_ast() {
        // tree-sitter's TypeScript grammar parses `fn?.()` as a plain
        // call_expression without an optional_chain field. We cannot
        // detect this form, so we don't count it.
        let source = "function invoke(fn: any) { return fn?.(); }";
        let tree = parse_ts(source);
        let func = tree.root_node().named_child(0).unwrap();
        // base only — optional call chaining not surfaced in AST
        assert_eq!(compute_cyclomatic_complexity(&func, source.as_bytes()), 1);
    }

    #[test]
    fn loc_counts_code_only_file() {
        let source = "const a = 1;\nconst b = 2;\nconst c = 3;\n";
        let tree = parse_ts(source);
        let counts = count_lines(source, &tree);
        assert_eq!(counts.total_lines, 3);
        assert_eq!(counts.code_lines, 3);
        assert_eq!(counts.comment_lines, 0);
        assert_eq!(counts.blank_lines, 0);
    }

    #[test]
    fn loc_counts_inline_comment_as_code() {
        let source = "const x = 1; // inline note\n";
        let tree = parse_ts(source);
        let counts = count_lines(source, &tree);
        assert_eq!(counts.total_lines, 1);
        assert_eq!(counts.code_lines, 1);
        assert_eq!(counts.comment_lines, 0);
    }

    #[test]
    fn loc_counts_multiline_block_comment() {
        let source = "/*\n * Multi\n * line\n */\nconst x = 1;\n";
        let tree = parse_ts(source);
        let counts = count_lines(source, &tree);
        assert_eq!(counts.total_lines, 5);
        assert_eq!(counts.code_lines, 1);
        assert_eq!(counts.comment_lines, 4);
    }

    #[test]
    fn zero_param_function_reports_zero() {
        let source = "function noArgs() { return 42; }";
        let tree = parse_ts(source);
        let funcs = extract_function_metrics(&tree, source);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].parameter_count, 0);
    }

    #[test]
    fn switch_default_does_not_add_complexity() {
        let source = r#"
function route(method: string) {
    switch (method) {
        case "GET":
            return handleGet();
        default:
            return handleOther();
    }
}
"#;
        let tree = parse_ts(source);
        let func = tree.root_node().named_child(0).unwrap();
        // 1 base + 1 switch_case (GET only, default excluded) = 2
        let complexity = compute_cyclomatic_complexity(&func, source.as_bytes());
        assert_eq!(complexity, 2);
    }
}
