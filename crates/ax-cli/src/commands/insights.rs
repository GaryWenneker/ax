use crate::commands::resolve_path;
use crate::ui::SpinnerGuard;

pub async fn run(
    path: Option<String>,
    resolution: f64,
    god_limit: usize,
    surprising_limit: usize,
    json: bool,
) -> Result<(), String> {
    let root = resolve_path(path);
    ax_usage::log_cli(Some(&root), "cmd=insights start");
    let resolution = if resolution > 0.0 { resolution } else { 1.0 };
    let insights = {
        let _spinner = SpinnerGuard::new("Analyzing graph structure...".to_string(), json);
        let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
        match ax.insights(resolution, god_limit, surprising_limit).await {
            Ok(i) => i,
            Err(e) => {
                ax_usage::log_cli(Some(&root), "cmd=insights fail");
                return Err(e.to_string());
            }
        }
    };
    ax_usage::log_cli(
        Some(&root),
        format!(
            "cmd=insights ok communities={}",
            insights.num_communities
        ),
    );

    if json {
        println!("{}", serde_json::to_string_pretty(&insights).unwrap_or_default());
        return Ok(());
    }

    println!("Graph insights for {}", root.display());
    println!(
        "  {} nodes · {} edges · {} communities · modularity {:.3}\n",
        insights.node_count, insights.edge_count, insights.num_communities, insights.modularity
    );

    println!("God nodes (most connected):");
    if insights.god_nodes.is_empty() {
        println!("  (none)");
    } else {
        for (i, g) in insights.god_nodes.iter().enumerate() {
            println!(
                "  {:>2}. {} [{}]  degree {} (in {}, out {})  {}",
                i + 1,
                g.name,
                g.kind,
                g.degree,
                g.in_degree,
                g.out_degree,
                g.file_path
            );
        }
    }

    println!("\nCommunities:");
    if insights.communities.is_empty() {
        println!("  (none)");
    } else {
        for c in insights.communities.iter().take(15) {
            let members = if c.key_nodes.is_empty() {
                String::new()
            } else {
                format!("  — {}", c.key_nodes.join(", "))
            };
            println!("  #{:<4} {:<28} {} nodes{}", c.community_id, c.label, c.size, members);
        }
        if insights.communities.len() > 15 {
            println!("  … and {} more", insights.communities.len() - 15);
        }
    }

    println!("\nSurprising connections (cross-community, cross-module):");
    if insights.surprising_connections.is_empty() {
        println!("  (none)");
    } else {
        for s in &insights.surprising_connections {
            println!(
                "  {} → {}  ({}, {})  [{} ⇄ {}]",
                s.source_name, s.target_name, s.kind, s.confidence, s.source_module, s.target_module
            );
        }
    }

    Ok(())
}
