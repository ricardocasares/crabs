use sqlx_custom_entities::{build_app, build_auth, connect_and_migrate};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL environment variable required");

    let pool = connect_and_migrate(&database_url).await?;
    println!("[*] migrations applied");

    let auth = build_auth(pool).await?;
    println!("[*] plugins: {:?}", auth.plugin_names());

    let app = build_app(auth);

    println!("[*] better-auth-poem on the Vercel runtime — http://127.0.0.1:3000");
    println!("    POST /auth/sign-up/email   POST /auth/sign-in/email   GET /api/me (Bearer)");

    vercel_poem::run(app).await?;
    Ok(())
}
