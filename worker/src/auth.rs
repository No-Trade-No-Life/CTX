//! Auth Mini 令牌（Ed25519 JWKS）与 user API key 校验。
use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use worker::*;

use crate::db::Db;
use crate::http::ApiError;

#[derive(Clone, Debug)]
pub struct Principal {
    pub user_id: String,
    pub via_api_key: bool,
}

#[derive(Deserialize)]
struct Jwk {
    kid: String,
    x: String,
    #[serde(default)]
    kty: String,
    #[serde(default)]
    crv: String,
}

#[derive(Deserialize)]
struct JwkSet {
    keys: Vec<Jwk>,
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
    #[serde(default)]
    aud: serde_json::Value,
    #[serde(default)]
    exp: Option<i64>,
    #[serde(default)]
    iss: Option<String>,
}

fn decode(segment: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(segment).ok()
}

pub fn hash_secret(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn audience_matches(aud: &serde_json::Value, expected: &str) -> bool {
    match aud {
        serde_json::Value::String(s) => s == expected,
        serde_json::Value::Array(items) => items.iter().any(|v| v.as_str() == Some(expected)),
        _ => false,
    }
}

async fn verifying_key(issuer: &str, kid: &str) -> Result<VerifyingKey, ApiError> {
    let url = format!("{issuer}/jwks");
    let mut response = Fetch::Url(url.parse().map_err(|_| ApiError::Internal("bad jwks url".into()))?)
        .send()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let set: JwkSet = response.json().await.map_err(|e| ApiError::Internal(e.to_string()))?;
    let jwk = set
        .keys
        .into_iter()
        .find(|k| k.kid == kid && (k.kty.is_empty() || k.kty == "OKP") && (k.crv.is_empty() || k.crv == "Ed25519"))
        .ok_or_else(|| ApiError::Unauthorized("invalid or expired bearer token".to_owned()))?;
    let bytes = decode(&jwk.x).ok_or_else(|| ApiError::Unauthorized("invalid or expired bearer token".to_owned()))?;
    let array: [u8; 32] = bytes
        .try_into()
        .map_err(|_| ApiError::Unauthorized("invalid or expired bearer token".to_owned()))?;
    VerifyingKey::from_bytes(&array).map_err(|_| ApiError::Unauthorized("invalid or expired bearer token".to_owned()))
}

pub async fn verify_token(token: &str, issuer: &str, audience: &str) -> Result<Principal, ApiError> {
    let invalid = || ApiError::Unauthorized("invalid or expired bearer token".to_owned());
    let mut parts = token.split('.');
    let (Some(header), Some(payload), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(invalid());
    };
    #[derive(Deserialize)]
    struct Header {
        kid: String,
        #[serde(default)]
        alg: String,
    }
    let header: Header = serde_json::from_slice(&decode(header).ok_or_else(invalid)?).map_err(|_| invalid())?;
    if !header.alg.is_empty() && header.alg != "EdDSA" {
        return Err(invalid());
    }
    let claims: Claims = serde_json::from_slice(&decode(payload).ok_or_else(invalid)?).map_err(|_| invalid())?;
    if !audience_matches(&claims.aud, audience) {
        return Err(invalid());
    }
    if let Some(iss) = claims.iss.as_deref() {
        if iss.trim_end_matches('/') != issuer.trim_end_matches('/') {
            return Err(invalid());
        }
    }
    if let Some(exp) = claims.exp {
        if exp <= crate::db::now() {
            return Err(invalid());
        }
    }
    let key = verifying_key(issuer, &header.kid).await?;
    let sig_bytes = decode(signature).ok_or_else(invalid)?;
    let sig = Signature::from_slice(&sig_bytes).map_err(|_| invalid())?;
    let signing_input = format!("{header}.{payload}", header = token.split('.').next().unwrap_or_default());
    use ed25519_dalek::Verifier;
    key.verify(signing_input.as_bytes(), &sig).map_err(|_| invalid())?;
    Ok(Principal { user_id: claims.sub, via_api_key: false })
}

pub async fn principal(req: &Request, db: &Db, issuer: &str, audience: &str) -> Result<Principal, ApiError> {
    let token = req
        .headers()
        .get("authorization")
        .ok()
        .flatten()
        .and_then(|value| value.strip_prefix("Bearer ").map(str::to_owned))
        .filter(|token| !token.is_empty())
        .ok_or_else(|| ApiError::Unauthorized("invalid or expired bearer token".to_owned()))?;
    let Some(rest) = token.strip_prefix("ctx_") else {
        return verify_token(&token, issuer, audience).await;
    };
    let Some((user_id, secret)) = rest.split_once('_').filter(|(u, s)| !u.is_empty() && !s.is_empty()) else {
        return Err(ApiError::Unauthorized("invalid API key".to_owned()));
    };
    match db.authenticate_api_key(user_id, &hash_secret(secret)).await? {
        true => Ok(Principal { user_id: user_id.to_owned(), via_api_key: true }),
        false => Err(ApiError::Unauthorized("invalid API key".to_owned())),
    }
}
