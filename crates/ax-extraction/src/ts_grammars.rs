//! Tree-sitter grammar bindings for all supported languages.

use ax_types::Language;
use tree_sitter::Language as TsLanguage;

/// Returns the tree-sitter grammar for `lang`, if one is linked.
pub fn ts_language_for(lang: Language) -> Option<TsLanguage> {
    Some(match lang {
        Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        Language::Python => tree_sitter_python::LANGUAGE.into(),
        Language::Go => tree_sitter_go::LANGUAGE.into(),
        Language::Java => tree_sitter_java::LANGUAGE.into(),
        Language::Kotlin => tree_sitter_kotlin_ng::LANGUAGE.into(),
        Language::Csharp => tree_sitter_c_sharp::LANGUAGE.into(),
        Language::Javascript | Language::Jsx => tree_sitter_javascript::LANGUAGE.into(),
        Language::Typescript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        Language::C => tree_sitter_c::LANGUAGE.into(),
        Language::Cpp => tree_sitter_cpp::LANGUAGE.into(),
        Language::Php => tree_sitter_php::LANGUAGE_PHP.into(),
        Language::Ruby => tree_sitter_ruby::LANGUAGE.into(),
        Language::Swift => tree_sitter_swift::LANGUAGE.into(),
        Language::Dart => tree_sitter_dart::LANGUAGE.into(),
        Language::Scala => tree_sitter_scala::LANGUAGE.into(),
        Language::Lua => tree_sitter_lua::LANGUAGE.into(),
        Language::Luau => tree_sitter_luau::LANGUAGE.into(),
        Language::Objc => tree_sitter_objc::LANGUAGE.into(),
        Language::R => tree_sitter_r::LANGUAGE.into(),
        Language::Yaml => tree_sitter_yaml::LANGUAGE.into(),
        Language::Xml => tree_sitter_xml::LANGUAGE_XML.into(),
        Language::Vue => tree_sitter_vue_next::LANGUAGE.into(),
        Language::Svelte => tree_sitter_svelte_ng::LANGUAGE.into(),
        Language::Razor => tree_sitter_razor::LANGUAGE.into(),
        Language::Pascal => tree_sitter_pascal::LANGUAGE.into(),
        Language::Properties => tree_sitter_properties::LANGUAGE.into(),
        Language::Astro => tree_sitter_astro_next::LANGUAGE.into(),
        Language::Liquid => tree_sitter_jinja::language(),
        Language::Twig => tree_sitter_jinja_dialects::LANGUAGE.into(),
        Language::Unknown => return None,
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
    fn loads_all_linked_grammars() {
        for lang in Language::ALL {
            if lang == Language::Unknown {
                continue;
            }
            assert!(has_ts_grammar(lang), "{lang:?} must have a tree-sitter grammar");
        }
    }
}
