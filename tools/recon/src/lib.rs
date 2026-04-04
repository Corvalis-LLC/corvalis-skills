pub mod analyze;
pub mod cli;
pub mod complexity;
pub mod config;
pub mod deps;
pub mod language;
pub mod metrics;
pub mod output;
pub mod overview;
pub mod parse;
pub mod queries;
pub mod ranking;
pub mod resolve;
pub mod symbols;
pub mod walk;

use tree_sitter::Language;
use tree_sitter_language::LanguageFn;

unsafe extern "C" {
    fn tree_sitter_javascript() -> *const ();
    fn tree_sitter_typescript() -> *const ();
    fn tree_sitter_tsx() -> *const ();
    fn tree_sitter_svelte() -> *const ();
}

pub fn language_javascript() -> Language {
    let func = unsafe { LanguageFn::from_raw(tree_sitter_javascript) };
    Language::new(func)
}

pub fn language_typescript() -> Language {
    let func = unsafe { LanguageFn::from_raw(tree_sitter_typescript) };
    Language::new(func)
}

pub fn language_tsx() -> Language {
    let func = unsafe { LanguageFn::from_raw(tree_sitter_tsx) };
    Language::new(func)
}

pub fn language_svelte() -> Language {
    let func = unsafe { LanguageFn::from_raw(tree_sitter_svelte) };
    Language::new(func)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grammars_load_successfully() {
        let js = language_javascript();
        assert!(js.version() > 0);

        let ts = language_typescript();
        assert!(ts.version() > 0);

        let tsx = language_tsx();
        assert!(tsx.version() > 0);

        let svelte = language_svelte();
        assert!(svelte.version() > 0);
    }
}
