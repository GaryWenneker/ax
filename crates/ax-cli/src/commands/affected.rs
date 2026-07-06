use std::io::{self, Read};

use crate::commands::resolve_path;
use crate::ui::SpinnerGuard;

#[derive(Debug, clap::Args)]
pub struct AffectedArgs {
    pub files: Vec<String>,
    #[arg(long, help = "Read changed file paths from stdin")]
    pub stdin: bool,
    #[arg(long, default_value = "main", help = "Base branch for git diff when no files given")]
    pub base: String,
    #[arg(short, long, default_value = "5", help = "Max dependency traversal depth")]
    pub depth: u32,
    #[arg(short, long, help = "Glob filter for test files")]
    pub filter: Option<String>,
    #[arg(short, long, help = "Output as JSON")]
    pub json: bool,
    #[arg(short, long, help = "Output file paths only")]
    pub quiet: bool,
}

pub async fn run(args: AffectedArgs) -> Result<(), String> {
    let root = resolve_path(None);
    let _spinner = SpinnerGuard::new("Finding affected tests...", false);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;

    let changed = resolve_changed_files(&root, &args).await?;

    let mut tia_opts = ax_tia::TiaOptions::default().with_depth(args.depth);
    if let Some(ref pattern) = args.filter {
        tia_opts = ax_tia::TiaOptions::with_filter_pattern(pattern).map_err(|e| e.to_string())?;
        tia_opts = tia_opts.with_depth(args.depth);
    }

    let pool = ax.db_pool();
    let result = ax_tia::affected_files_from_changes(pool, &changed, &tia_opts)
        .await
        .map_err(|e| e.to_string())?;

    if args.quiet {
        for f in &result.test_files {
            println!("{f}");
        }
        return Ok(());
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&result).unwrap_or_default());
    } else {
        println!("Affected test files ({}):", result.test_files.len());
        for f in &result.test_files {
            println!("  {f}");
        }
        if !result.tests.is_empty() {
            println!("\nTest functions ({}):", result.tests.len());
            for t in &result.tests {
                println!("  {} — {}", t.qualified_name, t.runner_hint);
            }
        }
    }
    Ok(())
}

async fn resolve_changed_files(root: &std::path::Path, args: &AffectedArgs) -> Result<Vec<String>, String> {
    if args.stdin {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| e.to_string())?;
        return Ok(buf
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect());
    }
    if !args.files.is_empty() {
        return Ok(args.files.clone());
    }
    if let Ok(files) = ax_git::changed_files(root, &args.base) {
        if !files.is_empty() {
            return Ok(files);
        }
    }
    let ax = ax_core::Ax::open(root).await.map_err(|e| e.to_string())?;
    Ok(ax
        .get_pending_files()
        .await
        .into_iter()
        .map(|p| p.path)
        .collect())
}
