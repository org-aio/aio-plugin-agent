use az_agent_frontend::{RenderRequest, render};
use serde_json::json;
use std::io::{Read, Write};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let output = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "dist/frontend".into());
    if output == "--render" {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;
        let html = render(serde_json::from_str(&input)?).await?;
        std::io::stdout().write_all(html.as_bytes())?;
        return Ok(());
    }
    std::fs::create_dir_all(&output)?;
    for (file, page) in [
        ("index.html", "chat"),
        ("settings.html", "settings"),
        ("skills.html", "skills"),
    ] {
        let html = render(RenderRequest {
            section: "shell".into(),
            data: json!(page),
        })
        .await?;
        std::fs::write(format!("{output}/{file}"), html)?;
    }
    Ok(())
}
