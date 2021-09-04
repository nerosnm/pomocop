use std::{
    env::var,
    sync::{Arc, Mutex},
};

use pomocop::pomo::{PhaseResult, SessionConfig};
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, Instrument};

#[tokio::main]
async fn main() -> Result<(), pomocop::Error> {
    tracing_subscriber::fmt::init();
    dotenv::dotenv().ok();

    let config = SessionConfig::default().work(5).short(1).long(2);

    debug!(?config);

    let session = config.build();
    let id = session.id();

    let session_1 = Arc::new(Mutex::new(session));
    let session_2 = Arc::clone(&session_1);

    let run = tokio::spawn(
        async move {
            let phase = session_1.lock().unwrap().advance();

            info!("starting first phase");
            let mut result = phase.await;

            while let PhaseResult::Completed(_) | PhaseResult::Skipped(_) = result {
                info!(?result, "finished phase, starting next one");
                let phase = session_1.lock().unwrap().advance();
                result = phase.await;
            }

            match result {
                PhaseResult::Stopped(_) => info!(?result, "session stopped"),
                PhaseResult::Failed(_) => error!(?result, "session failed"),
                PhaseResult::Completed(_) | PhaseResult::Skipped(_) => unreachable!(),
            }
        }
        .instrument(tracing::info_span!("run", ?id)),
    );

    let mess_with = tokio::spawn(
        async move {
            sleep(Duration::from_secs(2)).await;

            info!("sending skip message");
            session_2.lock().unwrap().skip().expect("failed to skip");

            sleep(Duration::from_secs(62)).await;

            info!("sending stop message");
            session_2.lock().unwrap().stop().expect("failed to stop");
        }
        .instrument(tracing::info_span!("mess_with", ?id)),
    );

    tokio::try_join![run, mess_with]?;

    Ok(())

    // pomocop::start(
    //     var("APPLICATION_ID")?,
    //     var("OWNER_ID")?,
    //     var("PREFIX").unwrap_or_else(|_| "|".into()),
    //     var("TOKEN")?,
    // )
    // .await
}
