//! Grammar registry and extension mapping.

use std::collections::HashMap;

use ax_types::Language;

static EXTENSION_MAP: &[(&str, Language)] = &[
    (".ts", Language::Typescript),
    (".tsx", Language::Tsx),
    (".mts", Language::Typescript),
    (".cts", Language::Typescript),
    (".js", Language::Javascript),
    (".mjs", Language::Javascript),
    (".cjs", Language::Javascript),
    (".jsx", Language::Jsx),
    (".xsjs", Language::Javascript),
    (".xsjslib", Language::Javascript),
    (".py", Language::Python),
    (".pyw", Language::Python),
    (".go", Language::Go),
    (".rs", Language::Rust),
    (".java", Language::Java),
    (".c", Language::C),
    (".h", Language::C),
    (".cpp", Language::Cpp),
    (".cc", Language::Cpp),
    (".cxx", Language::Cpp),
    (".hpp", Language::Cpp),
    (".cs", Language::Csharp),
    (".cshtml", Language::Razor),
    (".razor", Language::Razor),
    (".php", Language::Php),
    (".module", Language::Php),
    (".install", Language::Php),
    (".rb", Language::Ruby),
    (".swift", Language::Swift),
    (".kt", Language::Kotlin),
    (".dart", Language::Dart),
    (".svelte", Language::Svelte),
    (".vue", Language::Vue),
    (".astro", Language::Astro),
    (".liquid", Language::Liquid),
    (".pas", Language::Pascal),
    (".scala", Language::Scala),
    (".lua", Language::Lua),
    (".luau", Language::Luau),
    (".m", Language::Objc),
    (".r", Language::R),
    (".yaml", Language::Yaml),
    (".yml", Language::Yaml),
    (".twig", Language::Twig),
    (".xml", Language::Xml),
    (".properties", Language::Properties),
];

pub fn extension_map() -> HashMap<String, Language> {
    EXTENSION_MAP
        .iter()
        .map(|(ext, lang)| (ext.to_string(), *lang))
        .collect()
}

pub fn language_for_extension(ext: &str) -> Option<Language> {
    let ext = if ext.starts_with('.') {
        ext.to_string()
    } else {
        format!(".{}", ext)
    };
    EXTENSION_MAP
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, l)| *l)
}

pub fn is_language_supported(lang: Language) -> bool {
  lang != Language::Unknown
}
