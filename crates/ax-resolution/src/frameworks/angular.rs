//! Angular route + component extraction (CG parity MVP).

use regex::Regex;

use ax_types::{NodeKind, ReferenceKind, UnresolvedReference};

use super::extract::{language_for_path, now_ms, stable_node_id, FrameworkExtractResult};

pub fn detect(project_root: &std::path::Path) -> bool {
    let pkg = project_root.join("package.json");
    if let Ok(content) = std::fs::read_to_string(&pkg) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            for key in ["dependencies", "devDependencies"] {
                if let Some(obj) = json.get(key).and_then(|d| d.as_object()) {
                    for k in obj.keys() {
                        if k.starts_with("@angular/") {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

pub fn extract_file(file_path: &str, content: &str) -> FrameworkExtractResult {
    if !file_path.ends_with(".ts") {
        return FrameworkExtractResult::default();
    }
    let lang = language_for_path(file_path);
    let mut out = FrameworkExtractResult::default();
    let now = now_ms();
    let route_path_re = Regex::new(r#"\bpath\s*:\s*['"]([^'"]*)['"]"#).expect("path");
    let component_re =
        Regex::new(r"\bcomponent\s*:\s*([A-Z][A-Za-z0-9_]*)").expect("component");
    let router_for_root_re = Regex::new(r"\bRouterModule\.forRoot\s*\(").expect("forRoot");

    if router_for_root_re.is_match(content) {
        for cap in route_path_re.captures_iter(content) {
            let path = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let line = content[..cap.get(0).unwrap().start()]
                .chars()
                .filter(|c| *c == '\n')
                .count() as i32
                + 1;
            let route_id = stable_node_id(file_path, &format!("route:{}", path));
            out.nodes.push(ax_types::Node {
                id: route_id.clone(),
                kind: NodeKind::Route,
                name: path.to_string(),
                qualified_name: format!("angular:{}", path),
                file_path: file_path.to_string(),
                language: lang,
                start_line: line,
                end_line: line,
                start_column: 0,
                end_column: 0,
                docstring: None,
                signature: None,
                visibility: None,
                is_exported: Some(true),
                is_async: None,
                is_static: None,
                is_abstract: None,
                decorators: None,
                type_parameters: None,
                return_type: None,
                updated_at: now,
            });
            if let Some(comp_cap) = component_re.captures(&content[cap.get(0).unwrap().start()..]) {
                let comp = comp_cap.get(1).map(|m| m.as_str()).unwrap_or("");
                if !comp.is_empty() {
                    out.references.push(UnresolvedReference {
                        from_node_id: route_id,
                        reference_name: comp.to_string(),
                        reference_kind: ReferenceKind::Calls,
                        line,
                        column: 0,
                        file_path: Some(file_path.to_string()),
                        language: Some(lang),
                        candidates: None,
                    });
                }
            }
        }
    }

  let selector_re = Regex::new(r#"selector\s*:\s*['"]([^'"]+)['"]"#).expect("selector");
    for cap in selector_re.captures_iter(content) {
        let selector = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        if selector.is_empty() {
            continue;
        }
        let line = content[..cap.get(0).unwrap().start()]
            .chars()
            .filter(|c| *c == '\n')
            .count() as i32
            + 1;
        let comp_id = stable_node_id(file_path, &format!("component:{}", selector));
        out.nodes.push(ax_types::Node {
            id: comp_id,
            kind: NodeKind::Component,
            name: selector.to_string(),
            qualified_name: selector.to_string(),
            file_path: file_path.to_string(),
            language: lang,
            start_line: line,
            end_line: line,
            start_column: 0,
            end_column: 0,
            docstring: None,
            signature: None,
            visibility: None,
            is_exported: Some(true),
            is_async: None,
            is_static: None,
            is_abstract: None,
            decorators: None,
            type_parameters: None,
            return_type: None,
            updated_at: now,
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_module_route_component() {
        let src = r#"
import { RouterModule } from '@angular/router';
const routes = RouterModule.forRoot([
  { path: 'home', component: HomePage },
]);
"#;
        let out = extract_file("app.routes.ts", src);
        assert!(!out.nodes.is_empty());
        assert!(out.references.iter().any(|r| r.reference_name == "HomePage"));
    }
}
