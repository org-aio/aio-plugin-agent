use az_agent_frontend::{RenderRequest, render};
use serde_json::json;

#[tokio::test]
async fn native_file_contents_and_names_cannot_inject_markup() {
    let html = render(RenderRequest {
        section: "dialog".into(),
        data: json!({"kind":"native-files","title":"项目文件","path":"/project/test.txt","file":true,"content":"<script>alert(1)</script><img src=x onerror=alert(2)>"}),
    }).await.unwrap();
    assert!(!html.contains("<script>"));
    assert!(!html.contains("<img src=x"));
    assert!(html.contains("&lt;script&gt;"));
}

#[tokio::test]
async fn message_html_and_untrusted_references_stay_inert() {
    let html = render(RenderRequest {
        section: "messages".into(),
        data: json!({"messages":[{
            "id":"x", "role":"assistant", "status":"cancelled",
            "content":"<script>alert(1)</script>\n\n<img src=x onerror=alert(2)>\n\n[bad](javascript:alert(3)) [fake](memory:unknown) [known](memory:approved) [external](https://example.org) [spoof](#memory:approved)",
            "citations":[{"id":"approved","title":"trusted"}],
            "error":"<svg onload=alert(4)>"
        }]}),
    }).await.unwrap();
    assert!(!html.contains("<script>"));
    assert!(!html.contains("<img src=x"));
    assert!(!html.contains("<svg onload"));
    assert!(!html.contains("href=\"javascript:"));
    assert!(!html.contains("href=\"#memory:unknown"));
    assert!(html.contains("href=\"#memory:approved\""));
    assert!(html.contains("href=\"https://example.org\""));
    assert!(html.contains("已停止"));
    assert!(html.contains(">trusted</a>"));
    assert!(!html.contains(">spoof</a>"));
}

#[tokio::test]
async fn unknown_fragments_are_rejected() {
    assert!(
        render(RenderRequest {
            section: "../settings".into(),
            data: json!({})
        })
        .await
        .is_err()
    );
}

#[tokio::test]
async fn separate_message_fragments_preserve_full_conversation() {
    let data = json!({
        "earlier":true,
        "messages":[{"id":"m1","role":"user","content":"hello"},{"id":"m2","role":"assistant","content":"world","details":{"status":"completed"}}],
        "pendingInput":{"id":"input"},
        "approvals":[{"id":42,"params":{"threadId":"t"}}]
    });
    let combined = render(RenderRequest {
        section: "messages".into(),
        data: data.clone(),
    })
    .await
    .unwrap();
    let mut pieces = render(RenderRequest {
        section: "message-header".into(),
        data: json!({"earlier":true,"empty":false}),
    })
    .await
    .unwrap();
    for item in data["messages"].as_array().unwrap() {
        pieces.push_str(
            &render(RenderRequest {
                section: "message".into(),
                data: item.clone(),
            })
            .await
            .unwrap(),
        );
    }
    pieces.push_str(
        &render(RenderRequest {
            section: "message-footer".into(),
            data,
        })
        .await
        .unwrap(),
    );
    assert_eq!(combined, pieces);
}
