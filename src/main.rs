use std::{
    env::var,
    sync::{Arc, Mutex},
};

use pomocop::pomo::SessionConfig;
use tokio::time::{sleep, Duration};
use tracing::{debug, info, Instrument};

#[tokio::main]
async fn main() -> Result<(), pomocop::Error> {
    tracing_subscriber::fmt::init();
    dotenv::dotenv().ok();

    let config = SessionConfig::default().work(1).short(1).long(1);

    debug!(?config);

    let session = config.build();
    let id = session.id();

    let session_1 = Arc::new(Mutex::new(session));
    let session_2 = Arc::clone(&session_1);

    let run = tokio::spawn(
        async move {
            info!("starting first phase");

            let phase = session_1.lock().expect("unable to lock session").advance();
            let result = phase.await;

            info!(?result, "finished first phase");
        }
        .instrument(tracing::info_span!("run", ?id)),
    );

    let stop = tokio::spawn(
        async move {
            sleep(Duration::from_secs(2)).await;

            info!("sending stop message");
            session_2
                .lock()
                .expect("unable to lock session")
                .stop()
                .expect("failed to stop");
        }
        .instrument(tracing::info_span!("stop", ?id)),
    );

    tokio::try_join![run, stop]?;

    Ok(())

    // pomocop::start(
    //     var("APPLICATION_ID")?,
    //     var("OWNER_ID")?,
    //     var("PREFIX").unwrap_or_else(|_| "|".into()),
    //     var("TOKEN")?,
    // )
    // .await
}
