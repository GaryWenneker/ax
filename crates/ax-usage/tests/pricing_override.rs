//! The user's `~/.ax/pricing.toml` override applies to the reference model in
//! the as-of-date lookup too, so daily savings bars use the same rate as the
//! headline total.
//!
//! Runs in its own test binary: the pricing config is loaded once per process
//! from `AX_HOME_DIR`, which this test points at a temporary home.

#[tokio::test]
async fn reference_price_as_of_honours_user_override() {
    let home = tempfile::tempdir().unwrap();
    let ax = home.path().join(".ax");
    std::fs::create_dir_all(&ax).unwrap();
    std::fs::write(
        ax.join("pricing.toml"),
        "reference_model = \"claude-sonnet\"\n\n[models.claude-sonnet]\ninput_per_mtok = 6.0\noutput_per_mtok = 30.0\n",
    )
    .unwrap();
    let db = home.path().join("usage.db");
    std::env::set_var("AX_HOME_DIR", home.path());
    std::env::set_var("AX_USAGE_DB", &db);

    // The synced catalog has the reference model at a different rate.
    let pool = ax_usage::open_pool().await.expect("pool");
    sqlx::query(
        "INSERT INTO model_price_daily (date, source, model_id, input_per_mtok, output_per_mtok)
         VALUES ('2026-07-26', 'openrouter', 'anthropic/claude-sonnet', 3.0, 15.0)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let total_rate = ax_usage::reference_pricing();
    assert_eq!(total_rate.input_per_mtok, 6.0, "headline uses the override");

    let (daily_rate, source) = ax_usage::price_as_of(None, "2026-07-26").await;
    assert_eq!(
        daily_rate.input_per_mtok, total_rate.input_per_mtok,
        "daily bars must use the same reference rate as the total (source {source})"
    );
    assert_eq!(source, "user");

    // A model with an override is unchanged: the override wins there too.
    let (model_rate, _) = ax_usage::price_as_of(Some("claude-sonnet-4"), "2026-07-26").await;
    assert_eq!(model_rate.input_per_mtok, 6.0);
}
