use anyhow::Result;
use serde_json::json;

use super::{
    configuration::Binding,
    model::{MAX_BYTES, MAX_SOURCE_BYTES, SourceRequest},
    util,
};

pub(super) fn request(language: &str, requirement: &str) -> Result<SourceRequest> {
    Ok(serde_json::from_value(json!({
        "requirement": requirement,
        "target": {"path":"/v1/math/double", "language":language, "source":"", "calls":[],
            "capabilities":[], "examples":[{"input":3,"output":6}]},
        "available": []
    }))?)
}

#[test]
fn generation_accepts_both_languages_and_only_source_output() -> Result<()> {
    for language in ["rhai", "typescript"] {
        let request = request(language, "将数字乘以二")?;
        let messages = util::messages(&request)?;
        assert_eq!(messages[0]["role"], "developer");
        let input: serde_json::Value = serde_json::from_str(
            messages[1]["content"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("缺少输入"))?,
        )?;
        assert_eq!(input["target"]["language"], language);
        assert_eq!(input["target"]["examples"], json!([{"input":3,"output":6}]));
    }
    assert_eq!(
        util::response(r#"{"source":"fn invoke(input, context) { input * 2 }"}"#)?.source,
        "fn invoke(input, context) { input * 2 }"
    );
    for invalid in [
        r#"{"source":"code","tenant_id":"another"}"#,
        r#"{"source":"one","source":"two"}"#,
        r#"{"source":" "}"#,
        "```json\n{\"source\":\"code\"}\n```",
        r#"{"revision":"activate-me"}"#,
    ] {
        assert!(util::response(invalid).is_err());
    }
    assert!(
        util::response(&json!({"source":"a".repeat(MAX_SOURCE_BYTES + 1)}).to_string()).is_err()
    );
    assert!(util::response(&"x".repeat(MAX_BYTES + 1)).is_err());
    assert!(util::messages(&request("javascript", "需求")?).is_err());
    assert!(util::messages(&request("rhai", " ")?).is_err());
    assert!(
        serde_json::from_value::<SourceRequest>(
            json!({"requirement":"x", "target":{}, "available":[], "user_id":"foreign"})
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn deployment_binding_requires_distinct_private_credentials_and_matching_tenant() -> Result<()> {
    use crate::transport::Ingress;
    let path =
        std::env::temp_dir().join(format!("aio-model-binding-{}.json", uuid::Uuid::new_v4()));
    let value = json!({"tenant_id":"tenant", "user_id":"owner", "provider_id":uuid::Uuid::new_v4(),
        "token":"model-fixture-only-0000000000000000", "public_origin":"http://127.0.0.1:4193"});
    std::fs::write(&path, serde_json::to_vec(&value)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    let mut ingress = Ingress {
        token: "separate-fixture-token".into(),
        tenant: Some("tenant".into()),
    };
    let result = (|| -> Result<()> {
        let binding = Binding::load(&path, &ingress)?;
        ingress.tenant = Some("foreign".into());
        assert!(Binding::load(&path, &ingress).is_err());
        ingress.tenant = None;
        ingress.token = binding.token.clone();
        assert!(Binding::load(&path, &ingress).is_err());
        let mut unsafe_binding = binding;
        unsafe_binding.public_origin = "http://model.example.test".into();
        assert!(unsafe_binding.validate().is_err());
        unsafe_binding.public_origin = "https://model.example.test/mcp".into();
        assert!(unsafe_binding.validate().is_err());
        unsafe_binding.public_origin = "https://model.example.test".into();
        assert!(unsafe_binding.validate().is_ok());
        Ok(())
    })();
    std::fs::remove_file(path)?;
    result
}
