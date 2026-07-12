//! Per-language tree-sitter node specs for generic extraction.

use ax_types::{Language, NodeKind};

use super::generic::{GenericExtractor, LangSpec};

macro_rules! spec {
    ($lang:expr, $exts:expr, $symbols:expr) => {
        LangSpec {
            language: $lang,
            extensions: $exts,
            symbols: $symbols,
            call_kinds: &[],
        }
    };
    ($lang:expr, $exts:expr, $symbols:expr, $calls:expr) => {
        LangSpec {
            language: $lang,
            extensions: $exts,
            symbols: $symbols,
            call_kinds: $calls,
        }
    };
}

static C_SPEC: LangSpec = spec!(
    Language::C,
    &[".c", ".h"],
    &[
        (NodeKind::Function, "function_definition"),
        (NodeKind::Struct, "struct_specifier"),
        (NodeKind::Enum, "enum_specifier"),
        (NodeKind::TypeAlias, "type_definition"),
    ],
    &["call_expression"]
);

static CPP_SPEC: LangSpec = spec!(
    Language::Cpp,
    &[".cpp", ".cc", ".cxx", ".hpp"],
    &[
        (NodeKind::Function, "function_definition"),
        (NodeKind::Class, "class_specifier"),
        (NodeKind::Struct, "struct_specifier"),
        (NodeKind::Namespace, "namespace_definition"),
    ],
    &["call_expression"]
);

static PHP_SPEC: LangSpec = spec!(
    Language::Php,
    &[".php", ".module", ".install"],
    &[
        (NodeKind::Function, "function_definition"),
        (NodeKind::Class, "class_declaration"),
        (NodeKind::Method, "method_declaration"),
    ],
    &["function_call_expression", "member_call_expression"]
);

static RUBY_SPEC: LangSpec = spec!(
    Language::Ruby,
    &[".rb"],
    &[
        (NodeKind::Method, "method"),
        (NodeKind::Class, "class"),
        (NodeKind::Module, "module"),
    ],
    &["call"]
);

static SWIFT_SPEC: LangSpec = spec!(
    Language::Swift,
    &[".swift"],
    &[
        (NodeKind::Class, "class_declaration"),
        (NodeKind::Protocol, "protocol_declaration"),
        (NodeKind::Function, "function_declaration"),
    ],
    &["call_expression"]
);

static DART_SPEC: LangSpec = spec!(
    Language::Dart,
    &[".dart"],
    &[
        (NodeKind::Class, "class_definition"),
        (NodeKind::Function, "function_signature"),
        (NodeKind::Enum, "enum_definition"),
    ],
    &["function_call_expression"]
);

static SCALA_SPEC: LangSpec = spec!(
    Language::Scala,
    &[".scala"],
    &[
        (NodeKind::Class, "class_definition"),
        (NodeKind::Class, "object_definition"),
        (NodeKind::Function, "function_definition"),
        (NodeKind::Trait, "trait_definition"),
    ],
    &["call_expression"]
);

static LUA_SPEC: LangSpec = spec!(
    Language::Lua,
    &[".lua"],
    &[
        (NodeKind::Function, "function_declaration"),
        (NodeKind::Function, "function_definition"),
    ],
    &["function_call"]
);

static LUAU_SPEC: LangSpec = spec!(
    Language::Luau,
    &[".luau"],
    &[
        (NodeKind::Function, "function_declaration"),
        (NodeKind::Function, "function_definition"),
    ],
    &["function_call"]
);

static OBJC_SPEC: LangSpec = spec!(
    Language::Objc,
    &[".m"],
    &[
        (NodeKind::Class, "class_interface"),
        (NodeKind::Class, "class_implementation"),
        (NodeKind::Method, "method_definition"),
        (NodeKind::Function, "function_definition"),
    ],
    &["message_send_expression"]
);

static R_SPEC: LangSpec = spec!(
    Language::R,
    &[".r"],
    &[(NodeKind::Function, "function_definition")],
    &["call"]
);

static YAML_SPEC: LangSpec = spec!(
    Language::Yaml,
    &[".yaml", ".yml"],
    &[(NodeKind::Variable, "block_mapping_pair")],
    &[]
);

static XML_SPEC: LangSpec = spec!(
    Language::Xml,
    &[".xml"],
    &[(NodeKind::Component, "element")],
    &[]
);

static VUE_SPEC: LangSpec = spec!(
    Language::Vue,
    &[".vue"],
    &[
        (NodeKind::Function, "function_declaration"),
        (NodeKind::Class, "class_declaration"),
    ],
    &["call_expression"]
);

static SVELTE_SPEC: LangSpec = spec!(
    Language::Svelte,
    &[".svelte"],
    &[
        (NodeKind::Function, "function_declaration"),
        (NodeKind::Class, "class_declaration"),
    ],
    &["call_expression"]
);

static RAZOR_SPEC: LangSpec = spec!(
    Language::Razor,
    &[".cshtml", ".razor"],
    &[
        (NodeKind::Class, "class_declaration"),
        (NodeKind::Method, "method_declaration"),
        (NodeKind::Function, "function_definition"),
    ],
    &["invocation_expression"]
);

static PASCAL_SPEC: LangSpec = spec!(
    Language::Pascal,
    &[".pas"],
    &[
        (NodeKind::Function, "function_declaration"),
        (NodeKind::Function, "procedure_declaration"),
        (NodeKind::Class, "class_type"),
    ],
    &["call_expression"]
);

static PROPERTIES_SPEC: LangSpec = spec!(
    Language::Properties,
    &[".properties"],
    &[(NodeKind::Variable, "key")],
    &[]
);

static ASTRO_SPEC: LangSpec = spec!(
    Language::Astro,
    &[".astro"],
    &[
        (NodeKind::Function, "function_declaration"),
        (NodeKind::Component, "component"),
    ],
    &["call_expression"]
);

static LIQUID_SPEC: LangSpec = spec!(
    Language::Liquid,
    &[".liquid"],
    &[
        (NodeKind::Variable, "assign"),
        (NodeKind::Function, "filter"),
    ],
    &[]
);

static TWIG_SPEC: LangSpec = spec!(
    Language::Twig,
    &[".twig"],
    &[
        (NodeKind::Function, "function"),
        (NodeKind::Class, "class"),
        (NodeKind::Variable, "set"),
    ],
    &["function_call"]
);

static ALL_SPECS: &[&LangSpec] = &[
    &C_SPEC,
    &CPP_SPEC,
    &PHP_SPEC,
    &RUBY_SPEC,
    &SWIFT_SPEC,
    &DART_SPEC,
    &SCALA_SPEC,
    &LUA_SPEC,
    &LUAU_SPEC,
    &OBJC_SPEC,
    &R_SPEC,
    &YAML_SPEC,
    &XML_SPEC,
    &VUE_SPEC,
    &SVELTE_SPEC,
    &RAZOR_SPEC,
    &PASCAL_SPEC,
    &PROPERTIES_SPEC,
    &ASTRO_SPEC,
    &LIQUID_SPEC,
    &TWIG_SPEC,
];

pub fn generic_extractors() -> Vec<GenericExtractor> {
    ALL_SPECS.iter().map(|s| GenericExtractor { spec: s }).collect()
}
