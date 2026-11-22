//! Proof that detection and verification answer per dialect.

use super::*;

use axum::{Json, Router, routing::get};
use serde_json::json;

/// A stand-in instance: only the documents this module asks for.
async fn stub(routes: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, routes).await;
    });
    format!("http://{addr}")
}

fn http() -> reqwest::Client {
    reqwest::Client::new()
}

#[tokio::test]
async fn a_misskey_instance_is_detected_and_its_token_proven() {
    let app = Router::new()
        .route(
            "/api/meta",
            get(|| async { Json(json!({ "maxNoteTextLength": 3000 })) }),
        )
        .route(
            "/api/i",
            axum::routing::post(
                |axum::Json(body): axum::Json<serde_json::Value>| async move {
                    assert_eq!(body["i"], "misskey-token");
                    Json(json!({
                        "id": "9abc",
                        "username": "ada",
                        "name": "Ada Lovelace",
                        "avatarUrl": "https://misskey.test/ada.png"
                    }))
                },
            ),
        );
    let own_base = stub(app).await;
    assert_eq!(
        detect_with(&http(), &own_base).await,
        Some(Protocol::Misskey)
    );
    let verified = verify_with(&http(), &own_base, "misskey-token", Protocol::Misskey)
        .await
        .unwrap();
    assert_eq!(verified.protocol, Protocol::Misskey);
    assert_eq!(verified.account_id, "9abc");
    assert_eq!(verified.display_name, "Ada Lovelace");
    assert_eq!(verified.avatar, "https://misskey.test/ada.png");
    assert_eq!(
        verified.acct,
        format!("ada@{}", own_base.trim_start_matches("http://"))
    );
    assert!(!Protocol::Misskey.reads_mastodon_api());
}

#[tokio::test]
async fn a_server_that_refuses_the_token_is_refused_for_misskey_and_mastodon() {
    let refusing = stub(Router::new().route(
        "/api/i",
        axum::routing::post(|| async {
            (
                axum::http::StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "invalid token" })),
            )
        }),
    ))
    .await;
    let misskey = verify_with(&http(), &refusing, "wrong", Protocol::Misskey)
        .await
        .unwrap_err();
    assert!(misskey.contains("401"), "{misskey}");
    assert!(misskey.contains("/api/i"), "{misskey}");

    let mastodon = verify_with(&http(), &refusing, "wrong", Protocol::Mastodon)
        .await
        .unwrap_err();
    assert!(mastodon.contains("404"), "{mastodon}");

    // A host that answers nothing is a refused token too, never an accepted one.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let closed = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    assert!(
        verify_with(&http(), &closed, "any", Protocol::Mastodon)
            .await
            .is_err()
    );
    assert_eq!(detect_with(&http(), &closed).await, None);
}

#[tokio::test]
async fn a_mastodon_instance_is_detected_and_a_nodus_one_wins() {
    let mastodon = stub(Router::new().route(
        "/api/v2/instance",
        get(|| async { Json(json!({ "version": "4.3.0" })) }),
    ))
    .await;
    assert_eq!(
        detect_with(&http(), &mastodon).await,
        Some(Protocol::Mastodon)
    );

    let nodus = stub(Router::new().route(
        "/api/v1/nodus/capabilities",
        get(|| async { Json(json!({ "contract": "0.1.0", "features": ["batch"] })) }),
    ))
    .await;
    assert_eq!(detect_with(&http(), &nodus).await, Some(Protocol::Nodus));
    assert!(Protocol::Nodus.reads_mastodon_api());
}
