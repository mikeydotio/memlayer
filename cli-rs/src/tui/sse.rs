use futures::StreamExt;
use reqwest_eventsource::{Event, EventSource};
use tokio::sync::mpsc;

use memlayer_common::api_types::StreamEntry;

use super::event::{AppEvent, SseConnectionStatus};

pub async fn run_sse_client(
    base_url: String,
    auth_token: String,
    tx: mpsc::UnboundedSender<AppEvent>,
) {
    loop {
        tx.send(AppEvent::SseStatus(SseConnectionStatus::Reconnecting))
            .ok();

        let client = reqwest::Client::new();
        let mut builder = client.get(format!("{base_url}/stream/entries"));
        if !auth_token.is_empty() {
            builder = builder.header("Authorization", format!("Bearer {auth_token}"));
        }

        let mut es = EventSource::new(builder).expect("Failed to create EventSource");

        while let Some(event) = es.next().await {
            match event {
                Ok(Event::Open) => {
                    tx.send(AppEvent::SseStatus(SseConnectionStatus::Connected))
                        .ok();
                }
                Ok(Event::Message(msg)) => {
                    if let Ok(entry) = serde_json::from_str::<StreamEntry>(&msg.data) {
                        tx.send(AppEvent::SseEntry(entry)).ok();
                    }
                }
                Err(reqwest_eventsource::Error::StreamEnded) => {
                    break;
                }
                Err(e) => {
                    tx.send(AppEvent::SseStatus(SseConnectionStatus::Disconnected(
                        format!("{e}"),
                    )))
                    .ok();
                    break;
                }
            }
        }

        // Wait before reconnecting
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}
