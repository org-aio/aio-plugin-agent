use super::model::*;
use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, OsRng, rand_core::RngCore},
};
use anyhow::{Result, anyhow, ensure};

pub fn text(value: &str, limit: usize, label: &str) -> ServiceResult<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > limit {
        return Err(bad(&format!("{label}长度应为 1 至 {limit}")));
    }
    Ok(value.into())
}
pub fn encrypt(key: &[u8; 32], secret: &str, owner: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| anyhow!("密钥无效"))?;
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            aes_gcm::aead::Payload {
                msg: secret.as_bytes(),
                aad: owner,
            },
        )
        .map_err(|_| anyhow!("密钥加密失败"))?;
    Ok([nonce.to_vec(), encrypted].concat())
}
pub fn decrypt(key: &[u8; 32], value: &[u8], owner: &[u8]) -> Result<String> {
    ensure!(value.len() >= 28, "密钥密文无效");
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| anyhow!("密钥无效"))?;
    let bytes = cipher
        .decrypt(
            Nonce::from_slice(&value[..12]),
            aes_gcm::aead::Payload {
                msg: &value[12..],
                aad: owner,
            },
        )
        .map_err(|_| anyhow!("密钥解密失败"))?;
    Ok(String::from_utf8(bytes)?)
}
pub fn owner(scope: &Scope, id: uuid::Uuid) -> Vec<u8> {
    serde_json::to_vec(&(&scope.tenant, &scope.user, id)).expect("字符串序列化")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secrets_are_bound_to_owner() {
        let encrypted = encrypt(&[7; 32], "test-secret", b"a").unwrap();
        assert_eq!(decrypt(&[7; 32], &encrypted, b"a").unwrap(), "test-secret");
        assert!(decrypt(&[7; 32], &encrypted, b"b").is_err());
        assert!(decrypt(&[8; 32], &encrypted, b"a").is_err());
    }
}
