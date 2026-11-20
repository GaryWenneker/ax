//! Print a stored spec or plan body.

use std::path::PathBuf;

pub async fn show(spec: String) -> Result<(), String> {
    let root = project_root()?;
    let database = ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .map_err(|error| error.to_string())?;
    let body = ax_usage::show_spec(database.pool(), &spec).await?;
    println!("{body}");
    Ok(())
}

fn project_root() -> Result<PathBuf, String> {
    let mut dir = std::env::current_dir().map_err(|error| error.to_string())?;
    loop {
        if dir.join(".ax").join("ax.db").is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err("no ax project database found".into());
        }
    }
}
