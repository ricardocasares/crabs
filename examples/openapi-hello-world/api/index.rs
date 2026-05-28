use openapi_hello_world::build_app;

#[tokio::main]
async fn main() -> Result<(), vercel_poem::Error> {
    vercel_poem::run(build_app()).await
}
