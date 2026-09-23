//! Build stable documentation-catalog memory blocks.

use ax_memory::MemoryRow;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySummary {
    pub id: String,
    pub kind: String,
    pub title: String,
}

pub struct ScanData {
    pub name: String,
    pub skill: Option<String>,
    pub wiki_root_url: String,
    pub wiki_pages: usize,
    pub wiki_sections: Vec<String>,
    pub integration_pages: Vec<String>,
    pub products: Vec<String>,
    pub docs_sections: Vec<String>,
    pub skill_names: Vec<String>,
    pub script_readmes: Vec<String>,
    pub synced_at: i64,
}

pub fn build_memories(data: &ScanData) -> Vec<MemoryRow> {
    let name = &data.name;
    let products: Vec<&String> = data
        .products
        .iter()
        .filter(|p| !p.starts_with("--"))
        .collect();
    let product_list = products
        .iter()
        .take(20)
        .map(|p| p.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let docs_section_list = data.docs_sections.join(", ");
    let scripts_list = data.script_readmes.join(", ");
    let refresh = match &data.skill {
        Some(skill) => format!("ax docs-catalog sync (skill: {skill})"),
        None => "ax docs-catalog sync".into(),
    };
    let procedure = match &data.skill {
        Some(skill) => format!("Run: ax docs-catalog sync or skill {skill}."),
        None => "Run: ax docs-catalog sync.".into(),
    };
    let wiki_line = if data.wiki_root_url.is_empty() {
        "not configured (docsCatalog.wiki_remote)".to_string()
    } else {
        format!("{} ({} pages)", data.wiki_root_url, data.wiki_pages)
    };
    let sync_time = chrono_like_timestamp(data.synced_at);

    vec![
        catalog_memory(
            "d289a48c-9394-42a7-a679-8c3d3d754a4c",
            "architecture",
            &format!("{name} documentation catalog - master index (ax db)"),
            &format!(
                "{name} documentation catalog. CANONICAL INDEX in ax.db - tag documentation-catalog.\n\n\
Refresh: {refresh}\n\n\
Source tiers:\n\
1. Wiki: {wiki_line}\n\
2. Local .docs/: {} sections - {}\n\
3. Agent skills: {} in .agents/skills/\n\
4. Scripts with README: {}\n\n\
Agents: ax recall 'documentation catalog' before guessing doc paths.\n\
Last sync: {sync_time} UTC",
                data.docs_sections.len(),
                docs_section_list,
                data.skill_names.len(),
                scripts_list,
            ),
            vec![
                "documentation-catalog".into(),
                "wiki".into(),
                "team".into(),
                "onboarding".into(),
            ],
            vec![".docs/README.md".into()],
            data.synced_at,
        ),
        catalog_memory(
            "36eebf19-f6b7-4162-a0d9-6a180de0565d",
            "architecture",
            "Wiki - top-level structure",
            &format!(
                "Wiki: {}\n\nSections ({}): {}\nTotal wiki pages: {}",
                data.wiki_root_url,
                data.wiki_sections.len(),
                data.wiki_sections.join(", "),
                data.wiki_pages,
            ),
            vec!["documentation-catalog".into(), "wiki".into()],
            vec![],
            data.synced_at,
        ),
        catalog_memory(
            "152ce030-3b78-4759-9b71-c0534a210f86",
            "architecture",
            "Wiki - products",
            &format!("Products ({}): {}", products.len(), product_list),
            vec![
                "documentation-catalog".into(),
                "wiki".into(),
                "products".into(),
            ],
            vec![],
            data.synced_at,
        ),
        catalog_memory(
            "5ed08321-346a-47d4-9c5d-f600ab340d11",
            "architecture",
            "Wiki - integrations",
            &format!(
                "Wiki integration pages ({}): {}",
                data.integration_pages.len(),
                data.integration_pages.join(", "),
            ),
            vec![
                "documentation-catalog".into(),
                "wiki".into(),
                "integrations".into(),
            ],
            vec![],
            data.synced_at,
        ),
        catalog_memory(
            "1cb271f8-dd6c-48ae-b597-910e9c1bfe50",
            "architecture",
            "Local .docs sections",
            &format!(
                ".docs/ ({} sections): {}",
                data.docs_sections.len(),
                docs_section_list,
            ),
            vec!["documentation-catalog".into(), "docs".into()],
            vec![".docs/README.md".into()],
            data.synced_at,
        ),
        catalog_memory(
            "1690375d-dfaf-4055-821f-976cfeb123d2",
            "convention",
            &format!("{name} agent skills"),
            &format!(
                ".agents/skills/ ({}): {}",
                data.skill_names.len(),
                data.skill_names.join(", "),
            ),
            vec!["documentation-catalog".into(), "skills".into()],
            vec![],
            data.synced_at,
        ),
        catalog_memory(
            "b45da6ee-2308-4d11-9edf-de334791bc4b",
            "note",
            &format!("{name} scripts"),
            &format!(
                ".scripts/ README ({}): {}",
                data.script_readmes.len(),
                scripts_list,
            ),
            vec!["documentation-catalog".into(), "workspace".into()],
            vec![],
            data.synced_at,
        ),
        catalog_memory(
            "a8feda9f-9bba-4ed3-875e-19f60391d81b",
            "convention",
            "Documentation catalog sync procedure",
            &procedure,
            vec![
                "documentation-catalog".into(),
                "wiki".into(),
                "refresh".into(),
            ],
            vec![],
            data.synced_at,
        ),
    ]
}

fn catalog_memory(
    id: &str,
    kind: &str,
    title: &str,
    body: &str,
    tags: Vec<String>,
    files: Vec<String>,
    synced_at: i64,
) -> MemoryRow {
    MemoryRow {
        id: id.into(),
        kind: kind.into(),
        title: title.into(),
        body: body.into(),
        tags,
        files,
        confidence: 1.0,
        source: "sync-script".into(),
        enabled: true,
        created_at: synced_at,
        updated_at: synced_at,
    }
}

fn chrono_like_timestamp(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(skill: Option<&str>) -> ScanData {
        ScanData {
            name: "Contoso".into(),
            skill: skill.map(String::from),
            wiki_root_url: "https://example.com/wiki".into(),
            wiki_pages: 12,
            wiki_sections: vec!["Apps/".into(), "Home.md".into()],
            integration_pages: vec!["Payments".into()],
            products: vec!["Contoso Portal".into(), "-- retired".into()],
            docs_sections: vec!["guides/".into()],
            skill_names: vec!["pr".into()],
            script_readmes: vec!["wcag/".into()],
            synced_at: 0,
        }
    }

    #[test]
    fn titles_are_generic_and_use_the_catalog_name() {
        let titles: Vec<String> = build_memories(&data(None))
            .into_iter()
            .map(|m| m.title)
            .collect();
        assert_eq!(
            titles,
            vec![
                "Contoso documentation catalog - master index (ax db)",
                "Wiki - top-level structure",
                "Wiki - products",
                "Wiki - integrations",
                "Local .docs sections",
                "Contoso agent skills",
                "Contoso scripts",
                "Documentation catalog sync procedure",
            ]
        );
    }

    #[test]
    fn ids_are_stable_so_existing_rows_update_in_place() {
        let ids: Vec<String> = build_memories(&data(None))
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(
            ids,
            vec![
                "d289a48c-9394-42a7-a679-8c3d3d754a4c",
                "36eebf19-f6b7-4162-a0d9-6a180de0565d",
                "152ce030-3b78-4759-9b71-c0534a210f86",
                "5ed08321-346a-47d4-9c5d-f600ab340d11",
                "1cb271f8-dd6c-48ae-b597-910e9c1bfe50",
                "1690375d-dfaf-4055-821f-976cfeb123d2",
                "b45da6ee-2308-4d11-9edf-de334791bc4b",
                "a8feda9f-9bba-4ed3-875e-19f60391d81b",
            ]
        );
    }

    #[test]
    fn master_index_tags_are_generic() {
        let master = &build_memories(&data(None))[0];
        assert_eq!(
            master.tags,
            vec!["documentation-catalog", "wiki", "team", "onboarding"]
        );
    }

    #[test]
    fn skill_is_mentioned_only_when_configured() {
        let without = build_memories(&data(None));
        assert!(without
            .iter()
            .all(|m| !m.body.contains("skill:") && !m.body.contains("skill ")));
        assert!(without
            .iter()
            .all(|m| m.files.iter().all(|f| !f.contains(".agents/skills/"))));

        let with = build_memories(&data(Some("contoso-docs")));
        assert!(with[0]
            .body
            .contains("ax docs-catalog sync (skill: contoso-docs)"));
        assert!(with[7].body.contains("skill contoso-docs"));
    }

    #[test]
    fn master_index_says_when_no_wiki_is_configured() {
        let mut d = data(None);
        d.wiki_root_url = String::new();
        let body = &build_memories(&d)[0].body;
        assert!(
            body.contains("1. Wiki: not configured (docsCatalog.wiki_remote)\n"),
            "{body}"
        );
    }

    #[test]
    fn scan_data_reaches_the_bodies() {
        let rows = build_memories(&data(None));
        assert!(rows[0].body.contains("https://example.com/wiki (12 pages)"));
        assert!(rows[1].body.contains("Apps/, Home.md"));
        assert_eq!(rows[2].body, "Products (1): Contoso Portal");
        assert!(rows[3].body.contains("(1): Payments"));
    }
}
