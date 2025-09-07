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
                    msg: format!("Tried to Convert {value} to APIScope"),
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
                reason: format!("API Key was valid but is deleted. Hash: {hash}"),
            }),
        }),
        Some((id, _, expires_at, scope)) => {
            if expires_at < chrono::Utc::now() {
                return Err(LTZFError::Validation {
                    source: Box::new(crate::error::DataValidationError::Unauthorized {
                        reason: format!("API Key was valid but is expired. Hash: {hash}"),
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

    async fn auth_listing(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        query_params: &models::AuthListingQueryParams,
    ) -> Result<AuthListingResponse> {
        todo!()
    }

    async fn auth_listing_keytag(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::AuthListingKeytagPathParams,
    ) -> Result<AuthListingKeytagResponse> {
        todo!()
    }

    #[doc = "AuthDelete - DELETE /api/v2/auth"]
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

    #[doc = "AuthPost - POST /api/v2/auth"]
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
            tracing::warn!("Permissions Insufficient");
            return Ok(AuthPostResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        tracing::debug!("Key Creation Requested!");
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
}

#[async_trait]
impl Authentifizierung<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;
    async fn auth_rotate(
        &self,
        _method: &axum::http::Method,
        _host: &axum_extra::extract::Host,
        _cookies: &axum_extra::extract::CookieJar,
        claims: &Self::Claims,
    ) -> Result<AuthRotateResponse> {
        let mut tx = self.sqlx_db.begin().await?;
        let old_key_entry = sqlx::query!(
            "SELECT scope,value as named_scope, expires_at, created_at 
            FROM api_keys INNER JOIN api_scope ON scope=api_scope.id 
            WHERE api_keys.id = $1",
            claims.1
        )
        .fetch_one(&mut *tx)
        .await?;

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
        WHERE id = $1",
            claims.1,
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
        Ok(AuthRotateResponse::Status201_RotationSuccessful(
            RotationResponse {
                new_api_key: new_key,
                rotation_complete_date: rot_expiration_date,
            },
        ))
    }
    /// AuthStatus - GET /api/v2/auth/status
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

pub fn keytag_of(thing: &String) -> String {
    return thing.chars().take(16).collect();
}

#[cfg(test)]
mod auth_test {
    use axum::http::Method;
    use axum_extra::extract::{CookieJar, Host};
    use openapi::apis::authentifizierung::{AuthRotateResponse, AuthStatusResponse, Authentifizierung};
    use openapi::apis::authentifizierung_keyadder_schnittstellen::*;
    use openapi::apis::collector_schnittstellen_vorgang::CollectorSchnittstellenVorgang;
    use openapi::models::{self, AuthListingQueryParams};

    use crate::api::auth::keytag_of;
    use crate::utils::test::{generate, TestSetup};
    use crate::LTZFServer;

    async fn fetch_key_index(server: &LTZFServer, keytag: String) -> i32{
        fetch_key_row(server, keytag).await.id
    }

    struct KeyRow{
        id: i32,
        created_by: i32,
        deleted_by: Option<i32>,
        key_hash: String,
        created_at: chrono::DateTime<chrono::Utc>,
        expires_at: chrono::DateTime<chrono::Utc>,
        last_used: Option<chrono::DateTime<chrono::Utc>>,
        scope: i32,
        rotated_for: Option<i32>,
        salt: String,
        keytag: String,
    }
    async fn fetch_key_row(server: &LTZFServer, keytag: String) -> KeyRow {
        let mut tx = server.sqlx_db.begin().await.unwrap();
        let index = sqlx::query!("SELECT * FROM api_keys WHERE keytag = $1", keytag)
        .map(|r| KeyRow{
            id: r.id,
            created_by: r.created_by,
            deleted_by: r.deleted_by,
            key_hash: r.key_hash,
            created_at: r.created_at,
            expires_at: r.expires_at,
            last_used: r.last_used,
            scope: r.scope,
            rotated_for: r.rotated_for,
            salt: r.salt,
            keytag: r.keytag
        })
        .fetch_one(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        index
    }

    // GET /auth/status
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

        let original_key = match key {
            AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(key) => key,
            _ => panic!("Unexpected: Expected success"),
        };
        let original_key_idx = fetch_key_index(server, keytag_of(&original_key)).await;

        let resp = server
            .auth_status(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, original_key_idx),
            )
            .await;

        assert!(
            matches!(&resp, Ok(AuthStatusResponse::Status200_SuccessfullyRetrievedAPIKeyStatus(r)) 
            if r.expires_at - expiry_date < chrono::Duration::milliseconds(1) && r.scope == "keyadder" && !r.is_being_rotated),
            "Expected Successful response, got {resp:?}"
        );

        let rotstruct = server
            .auth_rotate(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, original_key_idx),
            )
            .await
            .unwrap();
        let fresh_key = match rotstruct {
            AuthRotateResponse::Status201_RotationSuccessful(rots) => rots.new_api_key,
            _ => unreachable!("Unreachable")
        };
        let fresh_key_index = fetch_key_index(server, keytag_of(&fresh_key));

        let key_status_rot = server
            .auth_status(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, original_key_idx),
            )
            .await;
        assert!(
            matches!(&key_status_rot,
                Ok(AuthStatusResponse::Status200_SuccessfullyRetrievedAPIKeyStatus(r)) if (r.expires_at - chrono::Utc::now()) - chrono::Duration::days(1)
                < chrono::Duration::seconds(1) && r.scope == "keyadder" && r.is_being_rotated
            ),
            "Expected Successful response, got {key_status_rot:?}"
        );

        scenario.teardown().await;
    }

    async fn fetch_key_status(server: &LTZFServer, index: i32) -> models::ApiKeyStatus {
        match server
            .auth_status(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, index),
            )
            .await.unwrap() {
                AuthStatusResponse::Status200_SuccessfullyRetrievedAPIKeyStatus(stat) => stat
            }
    }

    // POST /auth
    #[tokio::test]
    async fn test_generate_key() {
        let scenario = TestSetup::new("test_generate_key").await;
        let server = &scenario.server;
        // generate a key without proper permission
        {
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
        }

        // generate a key with proper permission
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
        matches!(
            resp,
            AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(_)
        );
        let key = match resp {
            AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(key) => key,
            _ => panic!("Expected authorized response"),
        };
        assert_eq!(key.len(), 64);
        assert!(key.starts_with("ltzf_"));

        scenario.teardown().await;
    }

    // GET /auth/keys
    #[tokio::test]
    async fn test_list_keys() {
        let scenario = TestSetup::new("test_list_keys").await;
        let server = &scenario.server;
        let mut keys = vec![];
        match server
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
            .await {
                Ok(AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(key)) => keys.push(key),
                _=> unreachable!()
        }
        match server
            .auth_post(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1),
                &models::CreateApiKey {
                    scope: "collector".to_string(),
                    expires_at: None,
                },
            )
            .await {
                Ok(AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(key)) => keys.push(key),
                _=> unreachable!()
        }
        match server
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
            .await {
                Ok(AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(key)) => keys.push(key),
                _=> unreachable!()
        }
        match server
            .auth_post(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1),
                &models::CreateApiKey {
                    scope: "admin".to_string(),
                    expires_at: None,
                },
            )
            .await {
                Ok(AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(key)) => keys.push(key),
                _=> unreachable!()
        } 
        // ------------------------------------------------------------------------------------------------------------
        let response = server.auth_listing(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1),
                &AuthListingQueryParams{
                    page:None,
                    per_page: None,
                    since: None,
                    until: None,
                }
        ).await;
        assert!(matches!(
            response, 
            Ok(AuthListingResponse::Status200_OK { body, ..})
            if body.clone().sort_by(|x, y| x.to_string().cmp(&y.to_string())) == keys.iter().map(|x| keytag_of(x)).collect::<Vec<_>>().sort()
        ));
        // insufficient permissions
        let response = server.auth_listing(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::Collector, 1),
                &AuthListingQueryParams{
                    page:None,
                    per_page: None,
                    since: None,
                    until: None,
                }
        ).await;
        assert!(matches!(response, Ok(AuthListingResponse::Status403_Forbidden { .. })));

        scenario.teardown().await;
    }

    // GET /auth/keys/{key_tag}
    #[tokio::test]
    async fn test_administer_key_details() {
        let scenario = TestSetup::new("test_administer_key_details").await;
        let server = &scenario.server;
        let rsp = server
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
            .await;
        let key = match rsp {
            Ok(AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(key))=>key,
            _=> unreachable!()
        };
        let key_idx = fetch_key_index(server, keytag_of(&key)).await;
        server.vorgang_put(
                &Method::PUT,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, key_idx),
                &models::VorgangPutHeaderParams{
                    x_scraper_id: uuid::Uuid::nil(),
                },
                &generate::default_vorgang()
            ).await;
        // enough setup, here it comes:
        let rsp = server.auth_listing_keytag(
                &Method::PUT,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1),
                &models::AuthListingKeytagPathParams{
                    keytag: keytag_of(&key)
                }
        ).await;
        match rsp {
            Ok(AuthListingKeytagResponse::Status200_OK { body, ..}) => {
                todo!("{:?}", body)
            },
            _ => unreachable!()
        }
        scenario.teardown().await;
    }

    // DELETE /auth
    #[tokio::test]
    async fn test_revoke_key() {
        let scenario = TestSetup::new("test_revoke_key").await;
        let server = &scenario.server;
        let rsp = server
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
        assert!(matches!(
            rsp,
            AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(..)
        ));
        let key = match rsp {
            AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(key) => key,
            _ => unreachable!("Not possible"),
        };
        let tag = keytag_of(&key);
        let row = fetch_key_row(server, tag.clone()).await;
        assert_eq!(row.deleted_by, None);

        // delete successfully
        let rsp = server
            .auth_delete(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1),
                &models::AuthDeleteHeaderParams {
                    api_key_delete: tag.clone(),
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            rsp,
            AuthDeleteResponse::Status204_NoContent {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            }
        ));
        let row = fetch_key_row(server, tag.clone()).await;
        let idx = fetch_key_index(server, tag.clone()).await;
        assert_eq!(row.deleted_by, Some(idx));

        // delete already deleted key
        let rsp = server
            .auth_delete(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1),
                &models::AuthDeleteHeaderParams {
                    api_key_delete: tag.clone(),
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            rsp,
            AuthDeleteResponse::Status204_NoContent {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            }
        ));
        let row = fetch_key_row(server, tag.clone()).await;
        let idx = fetch_key_index(server, tag.clone()).await;
        assert_eq!(row.deleted_by, Some(idx));

        // delete without permission
        let rsp = server
            .auth_delete(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::Admin, 1),
                &models::AuthDeleteHeaderParams {
                    api_key_delete: tag.clone(),
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            rsp,
            AuthDeleteResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            }
        ));

        scenario.teardown().await;
    }

    // POST /auth/rotate
    #[tokio::test]
    async fn test_rotate_own_key() {
        let scenario = TestSetup::new("test_rotate_own_key").await;
        let server = &scenario.server;
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
        let key0 = match resp {
            AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(key) => key,
            _ => unreachable!("blub"),
        };
        let key0_idx = fetch_key_index(server, keytag_of(&key0)).await;

        // first test: status of the key is not in rotation
        // then rotate: The key is in rotation at index 2
        let key1 = {
            let key_status = fetch_key_status(server, 2).await;
            assert!(!key_status.is_being_rotated && key_status.scope == "keyadder".to_string());
            let rrsp = server.auth_rotate(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 2)
            ).await.unwrap();
            assert!(matches!(rrsp, AuthRotateResponse::Status201_RotationSuccessful(..)));
            match rrsp {
                AuthRotateResponse::Status201_RotationSuccessful(s) => s,
                _ => unreachable!("not possible")
            }
            // state of the db: 
            // 1. superadmin key
            // 2. key in rotation (1d)
            // 3. freshly generated key (1y)
        }.new_api_key;
        let key1_idx = fetch_key_index(server, keytag_of(&key1)).await;
        
        // testing that the time limits work and a "standalone" rotation is possible
        let key0_status = fetch_key_status(server, key0_idx).await; // old key
        assert!(key0_status.is_being_rotated && key0_status.scope == "keyadder".to_string() && key0_status.expires_at.checked_sub_days(chrono::Days::new(1)).unwrap() <= chrono::Utc::now());
        let key1_status = fetch_key_status(server, key1_idx).await; // new key
        assert!(!key1_status.is_being_rotated && key1_status.scope == "keyadder".to_string() && key1_status.expires_at.checked_sub_months(chrono::Months::new(11)).unwrap() <= chrono::Utc::now());

        // rotate the new key and expect the key in rotation to be invalidated
        let key2 = {
            let rrsp = server.auth_rotate(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 3)
            ).await.unwrap();
            assert!(matches!(rrsp, AuthRotateResponse::Status201_RotationSuccessful(..)));
            match rrsp {
                AuthRotateResponse::Status201_RotationSuccessful(s) => s,
                _ => unreachable!("not possible")
            }
            // state of the db: 
            // 1. superadmin key
            // key0: ~~key in rotation (1d)~~ -> invalidated key
            // key1: ~~freshly generated key (1y)~~ -> key in rotation (1d)
            // key2: freshly generated key (1y)
        }.new_api_key;
        let key2_idx = fetch_key_index(server, keytag_of(&key2)).await;
        let key2_status = fetch_key_status(server, key2_idx).await; // new key
        assert!(!key2_status.is_being_rotated);
        let key0_row = fetch_key_row(server, key0).await;
        assert_eq!(key0_row.deleted_by, Some(key0_idx));
        let key1_status = fetch_key_status(server, key1_idx).await;
        assert!(key1_status.is_being_rotated);

        // rotate the key in rotation again and expect the previously generated fresh key to be invalidated.
        // the old key's lifetime does not change from the first rotation (otherwise it could be extended -> secu risk)
        let key3 = {
            let rrsp = server.auth_rotate(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 3)
            ).await.unwrap();
            assert!(matches!(rrsp, AuthRotateResponse::Status201_RotationSuccessful(..)));
            match rrsp {
                AuthRotateResponse::Status201_RotationSuccessful(s) => s,
                _ => unreachable!("not possible")
            }
            // state of the db: 
            // 1. superadmin key
            // key0: ~~key in rotation (1d)~~ -> invalidated
            // key1: ~~freshly generated key (1y)~~ -> key in rotation (original timestamp)
            // key2: ~~freshly generated key (1y)~~ -> invalidated
            // key3: freshly generated key (1y)
        }.new_api_key;
        let key3_idx = fetch_key_index(server, keytag_of(&key3)).await;
        let key3_status = fetch_key_status(server, key3_idx).await;
        let key1_status_new = fetch_key_status(server, key1_idx).await;
        let key2_row = fetch_key_row(server, keytag_of(&key2)).await;

        assert_eq!(key1_status_new.expires_at, key1_status.expires_at);
        assert!(!key3_status.is_being_rotated);
        assert_eq!(key2_row.deleted_by, Some(key2_idx));

        scenario.teardown().await;
    }
}
