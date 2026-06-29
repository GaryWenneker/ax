// Index-only fixture — Actix `web::resource` chain (CG rust.ts lines 221–245)
async fn legacy_get() {}

fn _actix_route_smoke() {
    web::resource("/legacy").route(web::get().to(legacy_get));
}