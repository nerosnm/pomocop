use std::env::var;

#[tokio::main]
async fn main() -> Result<(), pomocop::Error> {
    tracing_subscriber::fmt::init();
    dotenv::dotenv().ok();

    // let mess_with = tokio::spawn(
    //     async move {
    //         sleep(Duration::from_secs(2)).await;

    //         info!("sending skip message");
    //         session_2.lock().unwrap().skip().expect("failed to skip");

    //         sleep(Duration::from_secs(62)).await;

    //         info!("sending stop message");
    //         session_2.lock().unwrap().stop().expect("failed to stop");
    //     }
    //     .instrument(tracing::info_span!("mess_with", ?id)),
    // );

    pomocop::run(
        var("APPLICATION_ID")?,
        var("OWNER_ID")?,
        var("PREFIX").unwrap_or_else(|_| "|".into()),
        var("TOKEN")?,
    )
    .await
}
