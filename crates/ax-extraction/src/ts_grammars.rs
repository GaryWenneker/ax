//! Tree-sitter grammar bindings for all supported languages.
//!
//! With the `minimal-grammars` feature only the core set is referenced: Rust, Python, Go,
//! Java, JavaScript/JSX, TypeScript/TSX, C, PHP, Ruby, Lua, YAML, XML, Vue, Svelte, Astro,
//! Properties, Liquid and Twig. The other grammars (Kotlin, C#, C++, Swift, Dart, Scala,
//! Luau, Objective-C, R, Razor, Pascal) hold most of the parse-table data, so headless
//! builds leave them out.

use ax_types::Language;
use tree_sitter::Language as TsLanguage;

/// Returns the tree-sitter grammar for `lang`, if one is linked.
pub fn ts_language_for(lang: Language) -> Option<TsLanguage> {
    Some(match lang {
        Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        Language::Python => tree_sitter_python::LANGUAGE.into(),
        Language::Go => tree_sitter_go::LANGUAGE.into(),
        Language::Java => tree_sitter_java::LANGUAGE.into(),
        #[cfg(not(feature = "minimal-grammars"))]
        Language::Kotlin => tree_sitter_kotlin_ng::LANGUAGE.into(),
        #[cfg(not(feature = "minimal-grammars"))]
        Language::Csharp => tree_sitter_c_sharp::LANGUAGE.into(),
        Language::Javascript | Language::Jsx => tree_sitter_javascript::LANGUAGE.into(),
        Language::Typescript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        Language::C => tree_sitter_c::LANGUAGE.into(),
        #[cfg(not(feature = "minimal-grammars"))]
        Language::Cpp => tree_sitter_cpp::LANGUAGE.into(),
        Language::Php => tree_sitter_php::LANGUAGE_PHP.into(),
        Language::Ruby => tree_sitter_ruby::LANGUAGE.into(),
        #[cfg(not(feature = "minimal-grammars"))]
        Language::Swift => tree_sitter_swift::LANGUAGE.into(),
        #[cfg(not(feature = "minimal-grammars"))]
        Language::Dart => tree_sitter_dart::LANGUAGE.into(),
        #[cfg(not(feature = "minimal-grammars"))]
        Language::Scala => tree_sitter_scala::LANGUAGE.into(),
        Language::Lua => tree_sitter_lua::LANGUAGE.into(),
        #[cfg(not(feature = "minimal-grammars"))]
        Language::Luau => tree_sitter_luau::LANGUAGE.into(),
        #[cfg(not(feature = "minimal-grammars"))]
        Language::Objc => tree_sitter_objc::LANGUAGE.into(),
        #[cfg(not(feature = "minimal-grammars"))]
        Language::R => tree_sitter_r::LANGUAGE.into(),
        Language::Yaml => tree_sitter_yaml::LANGUAGE.into(),
        Language::Xml => tree_sitter_xml::LANGUAGE_XML.into(),
        Language::Vue => tree_sitter_vue_next::LANGUAGE.into(),
        Language::Svelte => tree_sitter_svelte_ng::LANGUAGE.into(),
        #[cfg(not(feature = "minimal-grammars"))]
        Language::Razor => tree_sitter_razor::LANGUAGE.into(),
        #[cfg(not(feature = "minimal-grammars"))]
        Language::Pascal => tree_sitter_pascal::LANGUAGE.into(),
        Language::Properties => tree_sitter_properties::LANGUAGE.into(),
        Language::Astro => tree_sitter_astro_next::LANGUAGE.into(),
        Language::Liquid => tree_sitter_jinja::language(),
        Language::Twig => tree_sitter_jinja_dialects::LANGUAGE.into(),
        Language::Unknown => return None,
        // Languages left out by `minimal-grammars`: files still get file nodes, no symbols.
        #[cfg(feature = "minimal-grammars")]
        _ => return None,
    })
}

pub fn has_ts_grammar(lang: Language) -> bool {
    ts_language_for(lang).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_types::Language;

    #[test]
    #[cfg(feature = "minimal-grammars")]
    fn minimal_grammars_keep_core_and_drop_heavy_languages() {
        assert!(has_ts_grammar(Language::Rust));
        assert!(has_ts_grammar(Language::Typescript));
        assert!(has_ts_grammar(Language::Python));
        assert!(!has_ts_grammar(Language::Razor));
        assert!(!has_ts_grammar(Language::Csharp));
    }

    #[test]
    #[cfg(feature = "minimal-grammars")]
    fn file_without_linked_grammar_parses_to_empty_result() {
        use crate::parse_pool::{ParsePool, ParseTask};
        let results = ParsePool::new().parse_batch(vec![ParseTask {
            file_path: "Greeter.cs".into(),
            content: "public class Greeter { public void Hello() { } }".into(),
            language: Language::Csharp,
        }]);
        let extraction = results[0]
            .1
            .as_ref()
            .expect("missing grammar is not an error");
        assert!(extraction.nodes.is_empty());
    }

    #[test]
    #[cfg(not(feature = "minimal-grammars"))]
    fn loads_all_linked_grammars() {
        for lang in Language::ALL {
            if lang == Language::Unknown {
                continue;
            }
            assert!(
                has_ts_grammar(lang),
                "{lang:?} must have a tree-sitter grammar"
            );
        }
    }
}
