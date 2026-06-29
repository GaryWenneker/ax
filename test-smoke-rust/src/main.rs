#[get("/users")]
async fn list_users() {}

#[post("/items")]
async fn create_item() {}

fn main() {
    Router::new().route("/health", get(health_check));
}

async fn health_check() {}