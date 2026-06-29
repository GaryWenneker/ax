//! Language extractors.

mod common;
mod go;
mod kotlin;
mod java;
mod javascript;
mod python;
mod refs;
mod rust;
mod typescript;

use std::collections::HashMap;

use ax_types::Language;

use crate::LanguageExtractor;

pub fn all_extractors() -> HashMap<Language, Box<dyn LanguageExtractor>> {
    let mut map: HashMap<Language, Box<dyn LanguageExtractor>> = HashMap::new();
    let extractors: Vec<Box<dyn LanguageExtractor>> = vec![
        Box::new(rust::RustExtractor),
        Box::new(python::PythonExtractor),
        Box::new(go::GoExtractor),
        Box::new(java::JavaExtractor),
        Box::new(kotlin::KotlinExtractor),
        Box::new(typescript::TypescriptExtractor),
        Box::new(javascript::JavascriptExtractor),
    ];
    for e in extractors {
        map.insert(e.language(), e);
    }
    map
}

pub fn extractor_for_language(lang: Language, extractors: &HashMap<Language, Box<dyn LanguageExtractor>>) -> Option<&dyn LanguageExtractor> {
    extractors.get(&lang).map(|b| b.as_ref())
}