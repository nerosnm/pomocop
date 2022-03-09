use std::env::var;

#[tokio::main]
async fn main() -> Result<(), pomocop::Error> {
    tracing_subscriber::fmt::init();
    dotenv::dotenv().ok();

    pomocop::run(
        var("APPLICATION_ID")?,
        var("OWNER_ID")?,
        var("PREFIX").unwrap_or_else(|_| "|".into()),
        var("TOKEN")?,
    )
    .await
}
