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
    pub wiki_root_url: String,
    pub wiki_pages: usize,
    pub wiki_sections: Vec<String>,
    pub integratie_pages: Vec<String>,
    pub digitale_producten: Vec<String>,
    pub docs_sections: Vec<String>,
    pub skill_names: Vec<String>,
    pub script_readmes: Vec<String>,
    pub synced_at: i64,
}

pub fn build_memories(data: &ScanData) -> Vec<MemoryRow> {
    let wiki_section_list = data.wiki_sections.join(", ");
    let integratie_list = data.integratie_pages.join(", ");
    let product_list: String = data
        .digitale_producten
        .iter()
        .filter(|p| !p.starts_with("--"))
        .take(20)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let docs_section_list = data.docs_sections.join(", ");
    let skills_list = data.skill_names.join(", ");
    let scripts_list = data.script_readmes.join(", ");

    let sync_time = chrono_like_timestamp(data.synced_at);

    vec![
        catalog_memory(
            "d289a48c-9394-42a7-a679-8c3d3d754a4c",
            "architecture",
            "VfPf documentation catalog - master index (ax db)",
            &format!(
                "VfPf documentation catalog (team: VfPf-NL Frontends). CANONICAL INDEX in ax.db - tag documentation-catalog.\n\n\
Refresh: ax docs-catalog sync (skill: vfpf-docs-catalog)\n\n\
Source tiers:\n\
1. AzDO wiki: {} - git: Frontends-Algemeen.wiki ({} pages)\n\
2. Local .docs/: {} sections - {}\n\
3. Agent skills: {} in .agents/skills/\n\
4. Scripts with README: {}\n\
5. Generated: .stories/, .release/, .reviews/, .incidents/, .status/, Documentation/\n\n\
Agents: ax recall 'documentation catalog' before guessing doc paths.\n\
Last sync: {sync_time} UTC",
                data.wiki_root_url,
                data.wiki_pages,
                data.docs_sections.len(),
                docs_section_list,
                data.skill_names.len(),
                scripts_list,
            ),
            vec![
                "documentation-catalog".into(),
                "vfpf".into(),
                "team".into(),
                "onboarding".into(),
            ],
            vec![
                ".ax/documentation-map.md".into(),
                ".docs/README.md".into(),
            ],
            data.synced_at,
        ),
        catalog_memory(
            "36eebf19-f6b7-4162-a0d9-6a180de0565d",
            "architecture",
            "AzDO wiki - Frontends-applicaties structure",
            &format!(
                "AzDO wiki Frontends-applicaties (752): {}\n\n\
Wiki sections ({}): {}\nTotal wiki pages: {}",
                data.wiki_root_url,
                data.wiki_sections.len(),
                wiki_section_list,
                data.wiki_pages,
            ),
            vec![
                "documentation-catalog".into(),
                "azdo-wiki".into(),
                "frontends-applicaties".into(),
            ],
            vec![],
            data.synced_at,
        ),
        catalog_memory(
            "152ce030-3b78-4759-9b71-c0534a210f86",
            "architecture",
            "AzDO wiki - Digitale Producten portfolio",
            &format!(
                "Products ({}): {}\n\
Repo map: Klantbeeld, AdviseurPortaal, New_Arbomeester, Pf_Portal, PPlein, Teamanalyse, Vfpf.nl, Arbocatalogus, Component-Library.",
                data.digitale_producten.len(),
                product_list,
            ),
            vec![
                "documentation-catalog".into(),
                "azdo-wiki".into(),
                "digitale-producten".into(),
            ],
            vec![],
            data.synced_at,
        ),
        catalog_memory(
            "5ed08321-346a-47d4-9c5d-f600ab340d11",
            "architecture",
            "AzDO wiki - Integraties + local mirror",
            &format!(
                "Wiki Integraties/ ({}): {}\nLocal: .docs/integrations/*.md",
                data.integratie_pages.len(),
                integratie_list,
            ),
            vec![
                "documentation-catalog".into(),
                "azdo-wiki".into(),
                "integraties".into(),
            ],
            vec![".docs/integrations/overview.md".into()],
            data.synced_at,
        ),
        catalog_memory(
            "e71911d7-e7d8-4c71-94f0-aa9d377b65a8",
            "note",
            "AzDO wiki - URLs per app and environment",
            "Wiki Frontends-applicaties/URLs.md - Dev/DevMaster/Test/Acc/Prod. Cross-check: lijstje skill, .proxy/Caddyfile.",
            vec![
                "documentation-catalog".into(),
                "azdo-wiki".into(),
                "urls".into(),
                "omgevingen".into(),
            ],
            vec![],
            data.synced_at,
        ),
        catalog_memory(
            "4b415cc7-fc97-4777-8e0d-530421eb0dfd",
            "architecture",
            "AzDO wiki - Azure DevOps section",
            "Wiki Azure-DevOps/: Projects, Repos, Pipelines, Agent-pool. Local: .docs/infrastructure/, vfpf-repo-policies.",
            vec![
                "documentation-catalog".into(),
                "azdo-wiki".into(),
                "azure-devops".into(),
            ],
            vec![],
            data.synced_at,
        ),
        catalog_memory(
            "64583eaf-779d-436e-a0e5-f9890a3e5689",
            "note",
            "AzDO wiki - Quality controls",
            "Wiki Quality-controls/: SonarQube, Dependency-Track, LCM. Local: build-gate, pre-pr-check.",
            vec![
                "documentation-catalog".into(),
                "azdo-wiki".into(),
                "quality".into(),
            ],
            vec![],
            data.synced_at,
        ),
        catalog_memory(
            "1cb271f8-dd6c-48ae-b597-910e9c1bfe50",
            "architecture",
            "Local .docs platform documentation sections",
            &format!(
                ".docs/ ({} sections): {}",
                data.docs_sections.len(),
                docs_section_list,
            ),
            vec!["documentation-catalog".into(), "docs".into()],
            vec![".docs/README.md".into(), ".docs/changelog.md".into()],
            data.synced_at,
        ),
        catalog_memory(
            "1690375d-dfaf-4055-821f-976cfeb123d2",
            "convention",
            "VfPf agent skills catalog",
            &format!(
                ".agents/skills/ ({}): {}",
                data.skill_names.len(),
                skills_list,
            ),
            vec!["documentation-catalog".into(), "skills".into()],
            vec![".agents/skills/vfpf-project/SKILL.md".into()],
            data.synced_at,
        ),
        catalog_memory(
            "b45da6ee-2308-4d11-9edf-de334791bc4b",
            "note",
            "VfPf workspace generated and script directories",
            &format!(
                ".scripts/ README ({}): {}",
                data.script_readmes.len(),
                scripts_list,
            ),
            vec!["documentation-catalog".into(), "workspace".into()],
            vec![
                ".scripts/wcag/README.md".into(),
                ".scripts/docs/Sync-DocumentationCatalog.ps1".into(),
            ],
            data.synced_at,
        ),
        catalog_memory(
            "a8feda9f-9bba-4ed3-875e-19f60391d81b",
            "convention",
            "Documentation catalog sync procedure",
            "Run: ax docs-catalog sync or skill vfpf-docs-catalog.",
            vec![
                "documentation-catalog".into(),
                "azdo-wiki".into(),
                "refresh".into(),
            ],
            vec![
                ".scripts/docs/Sync-DocumentationCatalog.ps1".into(),
                ".agents/skills/vfpf-docs-catalog/SKILL.md".into(),
            ],
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
