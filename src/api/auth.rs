use std::fmt::Display;

use crate::{LTZFServer, Result, error::LTZFError};
use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::CookieJar;
use axum_extra::extract::Host;
use openapi::apis::ApiKeyAuthHeader;
use openapi::apis::authentifizierung::*;
use openapi::apis::authentifizierung_keyadder_schnittstellen::AuthentifizierungKeyadderSchnittstellen;
use openapi::apis::authentifizierung_keyadder_schnittstellen::*;
use openapi::models;
use openapi::models::RotationResponse;
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
impl AuthentifizierungKeyadderSchnittstellen<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;
    #[doc = "AuthDelete - DELETE /api/v1/auth"]
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
            return Ok(AuthDeleteResponse::Status403_Forbidden {
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
            return Ok(AuthPostResponse::Status403_Forbidden {
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
        if claims.0 != APIScope::KeyAdder {
            return Ok(AuthRotateResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let mut tx = self.sqlx_db.begin().await?;
        let old_key_entry = sqlx::query!("SELECT scope,value as named_scope, expires_at, created_at FROM api_keys INNER JOIN api_scope ON scope=api_scope.id WHERE key_hash = $1", body.old_key_hash)
        .fetch_optional(&mut *tx)
        .await?;
        if old_key_entry.is_none() {
            tracing::warn!(
                "While rotating key: Expected to find old key with hash {} in the database",
                body.old_key_hash
            );
            return Ok(AuthRotateResponse::Status404_NotFound {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let old_key_entry = old_key_entry.unwrap();
        // new key, replacing the old one
        let new_key = generate_api_key().await;
        let key_digest = digest(new_key.clone());

        let new_id = sqlx::query!(
            "INSERT INTO api_keys(key_hash, created_by, expires_at, scope)
        VALUES
        ($1, $2, $3, $4)
        RETURNING id",
            key_digest,
            claims.1,
            chrono::Utc::now() + (old_key_entry.expires_at - old_key_entry.created_at),
            old_key_entry.scope
        )
        .map(|r| r.id)
        .fetch_one(&self.sqlx_db)
        .await?;

        let rot_expiration_date = chrono::Utc::now() + chrono::Duration::days(1);
        sqlx::query!(
            "UPDATE api_keys 
        SET expires_at = $2, rotated_for = $3
        WHERE key_hash = $1",
            body.old_key_hash,
            rot_expiration_date.clone(),
            new_id
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        tracing::info!(
            "Rotated API Key with Scope: {:?}",
            old_key_entry.named_scope
        );
        Ok(AuthRotateResponse::Status201_NewAPIKeyWasCreatedSuccessfullyWhilePreservingTheOldOneForTheTransitionPeriod(
            RotationResponse{
                new_api_key: new_key,
                rotation_complete_date: rot_expiration_date
            }
        ))
    }
}

#[async_trait]
impl Authentifizierung<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;
    /// AuthStatus - GET /api/v1/auth/status
    async fn auth_status(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
    ) -> Result<AuthStatusResponse> {
        tracing::debug!("Key {} ({}) requested auth status", claims.1, claims.0);
        let db_row = sqlx::query!(
            "SELECT * FROM 
        api_keys WHERE id = $1",
            claims.1
        )
        .fetch_one(&self.sqlx_db)
        .await?;
        let rotation_is_active =
            db_row.rotated_for.is_some() && db_row.expires_at > chrono::Utc::now();
        Ok(
            AuthStatusResponse::Status200_SuccessfullyRetrievedAPIKeyStatus(models::ApiKeyStatus {
                expires_at: db_row.expires_at,
                scope: claims.0.to_string(),
                is_being_rotated: rotation_is_active,
            }),
        )
    }
}

#[cfg(test)]
mod auth_test {
    use axum::http::Method;
    use axum_extra::extract::{CookieJar, Host};
    use openapi::apis::authentifizierung::{AuthStatusResponse, Authentifizierung};
    use openapi::apis::authentifizierung_keyadder_schnittstellen::*;
    use openapi::models;

    use crate::utils::test::TestSetup;
    #[tokio::test]
    async fn test_auth_status() {
        let scenario = crate::utils::test::TestSetup::new("test_auth_status").await;
        let server = &scenario.server;
        let expiry_date = chrono::Utc::now() + chrono::Duration::days(2);
        let key = server
            .auth_post(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1), // one is the default keyadder key
                &models::CreateApiKey {
                    expires_at: Some(expiry_date),
                    scope: super::APIScope::KeyAdder.to_string(),
                },
            )
            .await
            .unwrap();
        let key = match key {
            AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(key) => key,
            _ => panic!("Unexpected: Expected success"),
        };

        let resp = server
            .auth_status(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 2),
            )
            .await;
        assert!(
            matches!(&resp, Ok(AuthStatusResponse::Status200_SuccessfullyRetrievedAPIKeyStatus(r)) if r.expires_at - expiry_date < chrono::Duration::milliseconds(1) && r.scope == "keyadder" && !r.is_being_rotated),
            "Expected Successful response, got {:?}",
            resp
        );

        let _rot_key = server
            .auth_rotate(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 2),
                &models::AuthRotateRequest {
                    old_key_hash: sha256::digest(&key).to_string(),
                },
            )
            .await
            .unwrap();
        let key_status_rot = server
            .auth_status(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 2),
            )
            .await;
        assert!(
            matches!(&key_status_rot,
                Ok(AuthStatusResponse::Status200_SuccessfullyRetrievedAPIKeyStatus(r)) if (r.expires_at - chrono::Utc::now()) - chrono::Duration::days(1)
                < chrono::Duration::seconds(1) && r.scope == "keyadder" && r.is_being_rotated
            ),
            "Expected Successful response, got {:?}",
            key_status_rot
        );

        scenario.teardown().await;
    }

    #[tokio::test]
    async fn test_auth_rotate() {
        let scenario = TestSetup::new("test_auth_rot").await;
        let server = &scenario.server;
        let response = server
            .auth_rotate(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::Collector, 1),
                &models::AuthRotateRequest {
                    old_key_hash: "abc123abc123".to_string(),
                },
            )
            .await;
        assert!(
            matches!(
                &response,
                Ok(AuthRotateResponse::Status403_Forbidden { .. })
            ),
            "Expected to fail with too little permission"
        );
        // next: Not Found
        let response = server
            .auth_rotate(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1),
                &models::AuthRotateRequest {
                    old_key_hash: "abc123abc123".to_string(),
                },
            )
            .await;
        assert!(matches!(
            &response,
            Ok(AuthRotateResponse::Status404_NotFound { .. })
        ));

        // next: success!
        let key = server
            .auth_post(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1),
                &models::CreateApiKey {
                    scope: "keyadder".to_string(),
                    expires_at: None,
                },
            )
            .await
            .unwrap();
        let key = if let AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(key) = key {
            key
        } else {
            panic!("Expected Successful Key creation response")
        };
        let response = server
            .auth_rotate(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1),
                &models::AuthRotateRequest {
                    old_key_hash: sha256::digest(&key).to_string(),
                },
            )
            .await;
        assert!(matches!(&response,
            Ok(AuthRotateResponse::Status201_NewAPIKeyWasCreatedSuccessfullyWhilePreservingTheOldOneForTheTransitionPeriod(rotrsp)) if rotrsp.new_api_key != key && rotrsp.rotation_complete_date > chrono::Utc::now()
        ));
        scenario.teardown().await;
    }

    // Authentication tests
    #[tokio::test]
    async fn test_auth_auth() {
        let scenario = TestSetup::new("test_auth_post").await;
        let server = &scenario.server;

        let resp = server
            .auth_post(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::Collector, 1),
                &models::CreateApiKey {
                    scope: "admin".to_string(),
                    expires_at: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            resp,
            AuthPostResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            }
        );

        let resp = server
            .auth_post(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::Admin, 1),
                &models::CreateApiKey {
                    scope: "collector".to_string(),
                    expires_at: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            resp,
            AuthPostResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            }
        );

        let resp = server
            .auth_post(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1),
                &models::CreateApiKey {
                    scope: "keyadder".to_string(),
                    expires_at: None,
                },
            )
            .await
            .unwrap();
        assert!(match resp {
            AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(_) => {
                true
            }
            _ => false,
        });
        let key = match resp {
            AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(key) => key,
            _ => panic!("Expected authorized response"),
        };
        // delete
        let del = server
            .auth_delete(
                &Method::DELETE,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::Collector, 1),
                &models::AuthDeleteHeaderParams {
                    api_key_delete: key.clone(),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            del,
            AuthDeleteResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            }
        );
        let del = server
            .auth_delete(
                &Method::DELETE,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::Admin, 1),
                &models::AuthDeleteHeaderParams {
                    api_key_delete: key.clone(),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            del,
            AuthDeleteResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            }
        );

        let del = server
            .auth_delete(
                &Method::DELETE,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1),
                &models::AuthDeleteHeaderParams {
                    api_key_delete: "unknown-keyhash".to_string(),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            del,
            AuthDeleteResponse::Status404_NotFound {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            }
        );

        let del = server
            .auth_delete(
                &Method::DELETE,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1),
                &models::AuthDeleteHeaderParams {
                    api_key_delete: key,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            del,
            AuthDeleteResponse::Status204_NoContent {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            }
        );

        scenario.teardown().await;
    }
}
