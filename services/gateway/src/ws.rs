use std::collections::HashMap;

use actix_web::{HttpRequest, HttpResponse, web};
use actix_ws::Message;
use futures_util::StreamExt;
use nexium_core::jwt::JwtIssuer;
use serde::Deserialize;
use tokio::task::JoinHandle;

#[derive(Debug, Deserialize)]
struct WsRequest {
    op: String,
    channel: String,
    token: Option<String>,
}

pub async fn ws_handler(
    req: HttpRequest,
    stream: web::Payload,
    nats: web::Data<async_nats::Client>,
    issuer: web::Data<JwtIssuer>,
) -> Result<HttpResponse, actix_web::Error> {
    let (response, session, msg_stream) = actix_ws::handle(&req, stream)?;

    let nc = nats.get_ref().clone();
    let iss = issuer.get_ref().clone();

    actix_web::rt::spawn(connection_loop(session, msg_stream, nc, iss));

    Ok(response)
}

async fn connection_loop(
    mut session: actix_ws::Session,
    mut msg_stream: actix_ws::MessageStream,
    nats: async_nats::Client,
    issuer: JwtIssuer,
) {
    let mut subs: HashMap<String, JoinHandle<()>> = HashMap::new();

    while let Some(Ok(msg)) = msg_stream.next().await {
        match msg {
            Message::Text(text) => {
                handle_text(&text, &mut session, &nats, &issuer, &mut subs).await;
            }
            Message::Ping(bytes) => {
                let _ = session.pong(&bytes).await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    for (_, handle) in subs {
        handle.abort();
    }

    let _ = session.close(None).await;
}

async fn handle_text(
    text: &str,
    session: &mut actix_ws::Session,
    nats: &async_nats::Client,
    issuer: &JwtIssuer,
    subs: &mut HashMap<String, JoinHandle<()>>,
) {
    let req: WsRequest = match serde_json::from_str(text) {
        Ok(r) => r,
        Err(_) => {
            let _ = session.text(r#"{"error":"invalid message format"}"#).await;
            return;
        }
    };

    match req.op.as_str() {
        "subscribe" => subscribe(session, nats, issuer, subs, &req).await,
        "unsubscribe" => unsubscribe(session, subs, &req.channel).await,
        _ => {
            let _ = session.text(r#"{"error":"unknown op"}"#).await;
        }
    }
}

async fn subscribe(
    session: &mut actix_ws::Session,
    nats: &async_nats::Client,
    issuer: &JwtIssuer,
    subs: &mut HashMap<String, JoinHandle<()>>,
    req: &WsRequest,
) {
    if subs.contains_key(&req.channel) {
        let _ = session
            .text(format!(
                r#"{{"error":"already subscribed to {}"}}"#,
                req.channel
            ))
            .await;
        return;
    }

    let nats_topic = match resolve_nats_topic(&req.channel, req.token.as_deref(), issuer) {
        Ok(t) => t,
        Err(err) => {
            let _ = session.text(format!(r#"{{"error":"{err}"}}"#)).await;
            return;
        }
    };

    let mut nats_sub = match nats.subscribe(nats_topic).await {
        Ok(s) => s,
        Err(err) => {
            tracing::warn!(error = %err, channel = %req.channel, "nats subscribe failed");
            let _ = session.text(r#"{"error":"subscription failed"}"#).await;
            return;
        }
    };

    let channel_name = req.channel.clone();
    let mut session_clone = session.clone();

    let handle = actix_web::rt::spawn(async move {
        while let Some(nats_msg) = nats_sub.next().await {
            let data: serde_json::Value =
                serde_json::from_slice(&nats_msg.payload).unwrap_or_default();
            let envelope = serde_json::json!({
                "channel": channel_name,
                "data": data,
            });
            if session_clone.text(envelope.to_string()).await.is_err() {
                break;
            }
        }
    });

    subs.insert(req.channel.clone(), handle);

    let _ = session
        .text(format!(r#"{{"subscribed":"{}"}}"#, req.channel))
        .await;

    tracing::debug!(channel = %req.channel, "client subscribed");
}

async fn unsubscribe(
    session: &mut actix_ws::Session,
    subs: &mut HashMap<String, JoinHandle<()>>,
    channel: &str,
) {
    if let Some(handle) = subs.remove(channel) {
        handle.abort();
        let _ = session
            .text(format!(r#"{{"unsubscribed":"{channel}"}}"#))
            .await;
    } else {
        let _ = session
            .text(format!(r#"{{"error":"not subscribed to {channel}"}}"#))
            .await;
    }
}

fn resolve_nats_topic(
    channel: &str,
    token: Option<&str>,
    issuer: &JwtIssuer,
) -> Result<String, &'static str> {
    let pair_to_nats = |pair: &str| pair.replace('/', "-");

    if let Some(pair) = channel.strip_prefix("orderbook.") {
        Ok(format!("nexium.orderbook.{}", pair_to_nats(pair)))
    } else if let Some(pair) = channel.strip_prefix("trades.") {
        Ok(format!("nexium.trades.{}", pair_to_nats(pair)))
    } else if channel == "user.orders" {
        let raw_token = token
            .and_then(|t| t.strip_prefix("Bearer ").or(Some(t)))
            .ok_or("user.orders requires a token")?;

        let claims = issuer.verify(raw_token).map_err(|_| "invalid token")?;
        Ok(format!("nexium.orders.{}", claims.sub))
    } else {
        Err("unknown channel")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_issuer() -> JwtIssuer {
        JwtIssuer::new("test-secret-ws-gateway", 3600)
    }

    #[test]
    fn resolve_orderbook_channel() {
        let topic = resolve_nats_topic("orderbook.BTC/USDT", None, &test_issuer()).unwrap();
        assert_eq!(topic, "nexium.orderbook.BTC-USDT");
    }

    #[test]
    fn resolve_trades_channel() {
        let topic = resolve_nats_topic("trades.ETH/USDT", None, &test_issuer()).unwrap();
        assert_eq!(topic, "nexium.trades.ETH-USDT");
    }

    #[test]
    fn resolve_user_orders_without_token_fails() {
        let err = resolve_nats_topic("user.orders", None, &test_issuer()).unwrap_err();
        assert_eq!(err, "user.orders requires a token");
    }

    #[test]
    fn resolve_user_orders_with_valid_token() {
        let issuer = test_issuer();
        let user_id = uuid::Uuid::new_v4();
        let (token, _) = issuer.issue(user_id).unwrap();
        let topic =
            resolve_nats_topic("user.orders", Some(&format!("Bearer {token}")), &issuer).unwrap();
        assert_eq!(topic, format!("nexium.orders.{user_id}"));
    }

    #[test]
    fn resolve_user_orders_with_bad_token_fails() {
        let err = resolve_nats_topic("user.orders", Some("Bearer bad.token.here"), &test_issuer())
            .unwrap_err();
        assert_eq!(err, "invalid token");
    }

    #[test]
    fn resolve_unknown_channel_fails() {
        let err = resolve_nats_topic("foo.bar", None, &test_issuer()).unwrap_err();
        assert_eq!(err, "unknown channel");
    }
}
