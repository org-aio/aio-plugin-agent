use crate::generated::conversation::model::{Scope, ServiceResult, bad};
use sha2::{Digest, Sha256};

pub fn validate_path(path: &str) -> ServiceResult<()> {
    let parts: Vec<_> = path.split('/').collect();
    if path.len() > 512
        || parts.len() < 2
        || parts.len() > 16
        || parts.iter().any(|p| {
            p.is_empty()
                || p.starts_with('.')
                || p.contains(['\\', ':'])
                || p.chars().any(char::is_control)
                || matches!(
                    *p,
                    "node_modules" | "target" | "__pycache__" | "credentials.json" | "auth.json"
                )
                || p.ends_with(".pem")
                || p.ends_with(".key")
                || p.ends_with(".pyc")
        })
    {
        return Err(bad("Skill 路径无效或包含不应同步的文件"));
    }
    Ok(())
}
pub fn hash(bytes: &[u8], executable: bool) -> String {
    let mut hash = Sha256::new();
    hash.update([u8::from(executable)]);
    hash.update(bytes);
    format!("{:x}", hash.finalize())
}
pub fn owner(scope: &Scope, path: &str) -> Vec<u8> {
    serde_json::to_vec(&("skills", &scope.tenant, &scope.user, path)).expect("字符串序列化")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_cannot_escape_or_sync_credentials() {
        for path in [
            "../secret",
            "a/../../x",
            "/a/SKILL.md",
            "a/.git/config",
            "a/auth.json",
            "a/x.pem",
            "a\\b/x",
            "a/:x",
        ] {
            assert!(validate_path(path).is_err(), "{path}");
        }
        assert!(validate_path("rust/scripts/check.sh").is_ok());
        assert_ne!(hash(b"x", true), hash(b"x", false));
    }
}
