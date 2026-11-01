use std::collections::HashSet;
use std::time::Duration;

use futures_util::StreamExt;
use opencode_client::api::{OpenCodeApi, V2Adapter};
use opencode_client::event::sse::SseClient;
use opencode_client::server::{ServerConfig, ServerGuard};
use opencode_client::types::v2::keys as k;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::try_init().ok();

    // 1. RAII-сервер: авто-порт.
    let guard = ServerGuard::start(ServerConfig::default()).await?;
    let base = guard.base_url().to_string();
    println!("PASS: сервер {base}");

    // 2. Клиент (v2 — единственный протокол сервера opencode 2.x).
    let client = V2Adapter::new(base.clone(), None);
    assert!(client.health().await?);
    println!("PASS: health ok");

    // 3. SSE: /api/event отдаёт конверты {type, data}.
    let sse = SseClient::new(base, None);
    let mut stream = sse.stream().await?;
    println!("PASS: SSE подключён");

    // 4. Сессия.
    let sess = client.create_session(None).await?;
    let session_id = sess["id"].as_str().unwrap().to_string();
    println!("PASS: сессия {session_id}");

    // 5. Промпт.
    client
        .prompt_async(&session_id, "Ответь одним числом: 2+2=?", None)
        .await?;
    println!("PASS: промпт отправлен");

    // 6. Копим текст блоками: v2 не даёт part.id — блок текста это пара
    // (assistantMessageID, ordinal), дельты приходят в порядке следования.
    let mut assistant_messages: HashSet<String> = HashSet::new();
    let mut chunks: Vec<String> = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(25);
    let mut idle_for_this = false;

    while std::time::Instant::now() < deadline {
        match stream.next().await {
            Some(Ok(env)) => {
                let data = &env.data;
                let str_field = |name: &str| {
                    data.get(name).and_then(|v| v.as_str()).map(str::to_string)
                };
                match env.kind.as_str() {
                    // Шаг открывает ассистентское сообщение: только его текст
                    // попадает в ответ (как в v1 регистрация по message.updated).
                    k::STEP_STARTED | k::TEXT_STARTED => {
                        if let Some(mid) = str_field("assistantMessageID") {
                            assistant_messages.insert(mid);
                        }
                    }
                    k::TEXT_DELTA => {
                        let registered = str_field("assistantMessageID")
                            .is_some_and(|mid| assistant_messages.contains(&mid));
                        if registered {
                            if let Some(delta) = str_field("delta") {
                                chunks.push(delta);
                            }
                        }
                    }
                    // Конец хода: успех/прерывание/успокоение (см. TurnState::on_v2_idle).
                    k::IDLE | k::EXECUTION_SUCCEEDED | k::EXECUTION_INTERRUPTED => {
                        if data.get("sessionID").and_then(|v| v.as_str()) == Some(session_id.as_str())
                        {
                            idle_for_this = true;
                            break;
                        }
                    }
                    other => log::debug!("пример игнорирует событие {other}"),
                }
            }
            Some(Err(err)) => {
                eprintln!("sse err: {err}");
            }
            None => break,
        }
    }

    let text: String = chunks.concat();

    if idle_for_this && !text.trim().is_empty() {
        println!("PASS: ответ получен:");
        println!("    {text}");
    } else {
        println!("ANSWER(partial): >{text}<");
        println!("(idle={idle_for_this})");
    }

    println!("...закрываю (Drop убьёт сервер)");
    Ok(())
}
