use std::fmt::Display;

use crate::{LTZFServer, Result, error::LTZFError};
use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::CookieJar;
use axum_extra::extract::Host;
use openapi::apis::ApiKeyAuthHeader;
use openapi::apis::authentication::*;
use openapi::apis::authentication_keyadder_schnittstellen::*;
use openapi::models;
use rand::distr::Alphanumeric;
use rand::{Rng, rng};
use sha256::digest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum APIScope {
    KeyAdder,
    Admin,
    Collector,
}
impl TryFrom<&str> for APIScope {
    type Error = LTZFError;
    fn try_from(value: &str) -> Result<Self> {
        match value {
            "keyadder" => Ok(APIScope::KeyAdder),
            "admin" => Ok(APIScope::Admin),
            "collector" => Ok(APIScope::Collector),
            _ => Err(LTZFError::Validation {
                source: Box::new(crate::error::DataValidationError::InvalidEnumValue {
                    msg: format!("Tried to Convert {} to APIScope", value),
                }),
            }),
        }
    }
}
impl TryFrom<String> for APIScope {
    type Error = LTZFError;
    fn try_from(value: String) -> Result<Self> {
        APIScope::try_from(value.as_str())
    }
}
impl Display for APIScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            APIScope::KeyAdder => write!(f, "keyadder"),
            APIScope::Admin => write!(f, "admin"),
            APIScope::Collector => write!(f, "collector"),
        }
    }
}

pub async fn generate_api_key() -> String {
    let key: String = "ltzf_"
        .chars()
        .chain(rng().sample_iter(&Alphanumeric).take(59).map(char::from))
        .collect();
    key
}
async fn internal_extract_claims(
    server: &LTZFServer,
    headers: &axum::http::header::HeaderMap,
    key: &str,
) -> Result<crate::api::Claims> {
    let key = headers.get(key);
    if key.is_none() {
        return Err(LTZFError::Validation {
            source: Box::new(crate::error::DataValidationError::MissingField {
                field: "X-API-Key".to_string(),
            }),
        });
    }
    let key = key.unwrap().to_str()?;
    let hash = digest(key);
    tracing::trace!("Authenticating Key Hash {}", hash);
    let table_rec = sqlx::query!(
        "SELECT k.id, deleted, expires_at, value as scope 
        FROM api_keys k
        INNER JOIN api_scope s ON s.id = k.scope
        WHERE key_hash = $1",
        hash
    )
    .map(|r| (r.id, r.deleted, r.expires_at, r.scope))
    .fetch_optional(&server.sqlx_db)
    .await?;

    tracing::trace!("DB Result: {:?}", table_rec);
    match table_rec {
        Some((_, true, _, _)) => Err(LTZFError::Validation {
            source: Box::new(crate::error::DataValidationError::Unauthorized {
                reason: format!("API Key was valid but is deleted. Hash: {}", hash),
            }),
        }),
        Some((id, _, expires_at, scope)) => {
            if expires_at < chrono::Utc::now() {
                return Err(LTZFError::Validation {
                    source: Box::new(crate::error::DataValidationError::Unauthorized {
                        reason: format!("API Key was valid but is expired. Hash: {}", hash),
                    }),
                });
            }
            let scope = (APIScope::try_from(scope.as_str()).unwrap(), id);
            sqlx::query!(
                "UPDATE api_keys SET last_used = $1 WHERE key_hash = $2",
                chrono::Utc::now(),
                hash
            )
            .execute(&server.sqlx_db)
            .await?;
            tracing::trace!("Scope of key with hash`{}`: {:?}", hash, scope.0);
            Ok(scope)
        }
        None => Err(LTZFError::Validation {
            source: Box::new(crate::error::DataValidationError::Unauthorized {
                reason: "API Key was not found in the Database".to_string(),
            }),
        }),
    }
}

#[async_trait]
impl ApiKeyAuthHeader for LTZFServer {
    type Claims = crate::api::Claims;
    async fn extract_claims_from_header(
        &self,
        headers: &axum::http::header::HeaderMap,
        key: &str,
    ) -> Option<Self::Claims> {
        match internal_extract_claims(self, headers, key).await {
            Ok(claim) => Some(claim),
            Err(error) => {
                tracing::warn!("Authorization failed: {}", error);
                None
            }
        }
    }
}

#[async_trait]
impl AuthenticationKeyadderSchnittstellen<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;
    #[doc = "AuthDelete - DELETE /api/v1/auth"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn auth_delete(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        header_params: &models::AuthDeleteHeaderParams,
    ) -> Result<AuthDeleteResponse> {
        if claims.0 != APIScope::KeyAdder {
            return Ok(AuthDeleteResponse::Status403_AuthenticationFailed {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let hash = digest(&header_params.api_key_delete);
        let ret = sqlx::query!(
            "UPDATE api_keys SET deleted=TRUE WHERE key_hash=$1 RETURNING id",
            hash
        )
        .fetch_optional(&self.sqlx_db)
        .await?;

        if ret.is_some() {
            Ok(AuthDeleteResponse::Status204_NoContent {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            })
        } else {
            Ok(AuthDeleteResponse::Status404_NotFound {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            })
        }
    }

    #[doc = "AuthPost - POST /api/v1/auth"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn auth_post(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        body: &models::CreateApiKey,
    ) -> Result<AuthPostResponse> {
        if claims.0 != APIScope::KeyAdder {
            return Ok(AuthPostResponse::Status403_AuthenticationFailed {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let key = generate_api_key().await;
        let key_digest = digest(key.clone());

        sqlx::query!(
            "INSERT INTO api_keys(key_hash, created_by, expires_at, scope)
        VALUES
        ($1, $2, $3, (SELECT id FROM api_scope WHERE value = $4))",
            key_digest,
            claims.1,
            body.expires_at
                .unwrap_or(chrono::Utc::now() + chrono::Duration::days(365)),
            body.scope.to_string()
        )
        .execute(&self.sqlx_db)
        .await?;

        tracing::info!("Generated Fresh API Key with Scope: {:?}", body.scope);
        Ok(AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(
            key,
        ))
    }
    async fn auth_rotate(
        &self,
        _method: &axum::http::Method,
        _host: &axum_extra::extract::Host,
        _cookies: &axum_extra::extract::CookieJar,
        claims: &Self::Claims,
        body: &openapi::models::AuthRotateRequest,
    ) -> Result<AuthRotateResponse> {
        todo!()
    }
}
#[async_trait]
impl Authentication<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;
    /// AuthStatus - GET /api/v1/auth/status
    async fn auth_status(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
    ) -> Result<AuthStatusResponse> {
        todo!()
    }
}
