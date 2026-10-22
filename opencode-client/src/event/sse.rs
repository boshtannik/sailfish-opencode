use std::collections::VecDeque;
use std::pin::Pin;

use futures_util::{stream::unfold, Stream, StreamExt};
use reqwest::Client;

use crate::types::v2::EventEnvelope;

/// SSE client for the server's event stream (`GET /api/event`): the framing is
/// `data: <json>` lines plus `: heartbeat` comments.
pub struct SseClient {
    http: Client,
    base: String,
    auth: Option<(String, String)>,
}

impl SseClient {
    pub fn new(base: String, auth: Option<(String, String)>) -> Self {
        let builder = Client::builder().connect_timeout(std::time::Duration::from_secs(10));
        Self {
            http: builder.build().expect("reqwest client"),
            base,
            auth,
        }
    }

    /// Open the GET stream for `path` (e.g. `/api/event`), returning the
    /// chunk stream. Auth/status handling lives here, in one place.
    async fn open(
        &self,
        path: &str,
    ) -> Result<impl Stream<Item = Result<bytes::Bytes, reqwest::Error>>, String> {
        let url = format!("{}{}", self.base, path);
        let mut req = self.http.get(&url);
        if let Some((user, pass)) = &self.auth {
            req = req.basic_auth(user, Some(pass));
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("SSE status: {}", resp.status()));
        }
        Ok(resp.bytes_stream())
    }

    /// `GET /api/event` → a stream of [`EventEnvelope`] (see
    /// `vault/Разведка-v2.md` §2–§3).
    pub async fn stream(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<EventEnvelope, String>> + Send>>, String> {
        let chunks = self.open("/api/event").await?;
        Ok(Box::pin(framed_stream(chunks)))
    }
}

/// Turn the raw byte stream into a stream of `data:` frames parsed as `T`.
fn framed_stream<T>(
    chunks: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> impl Stream<Item = Result<T, String>> + Send
where
    T: serde::de::DeserializeOwned + Send + 'static,
{
    unfold(
        (Box::pin(chunks), Vec::new(), VecDeque::new()),
        move |(mut chunks, mut buffer, mut pending)| async move {
            loop {
                if let Some(ev) = pending.pop_front() {
                    return Some((ev, (chunks, buffer, pending)));
                }
                match chunks.next().await {
                    Some(Ok(bytes)) => {
                        buffer.extend_from_slice(&bytes);
                        parse_events(&mut buffer, &mut pending);
                    }
                    Some(Err(e)) => return Some((Err(e.to_string()), (chunks, buffer, pending))),
                    None => return None,
                }
            }
        },
    )
}

fn parse_events<T>(buffer: &mut Vec<u8>, out: &mut VecDeque<Result<T, String>>)
where
    T: serde::de::DeserializeOwned,
{
    // Parse only complete lines ending with \n
    let text = String::from_utf8_lossy(buffer);
    let mut complete_idx = 0;
    for line in text.split_inclusive('\n') {
        if line.ends_with('\n') {
            complete_idx += line.len();
            if let Some(data) = line.strip_prefix("data: ") {
                let data = data.trim_end_matches('\n');
                match serde_json::from_str::<T>(data) {
                    Ok(ev) => out.push_back(Ok(ev)),
                    Err(e) => out.push_back(Err(format!("parse: {e}"))),
                }
            }
        } else {
            break;
        }
    }
    if complete_idx > 0 {
        buffer.drain(..complete_idx);
    }
}
