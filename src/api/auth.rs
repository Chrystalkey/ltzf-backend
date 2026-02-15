use std::fmt::Display;

use crate::api::PaginationResponsePart;
use crate::utils::as_option;
use crate::{LTZFServer, Result, error::LTZFError};
use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::CookieJar;
use headers::Host;
use openapi::apis::ApiKeyAuthHeader;
use openapi::apis::authentifizierung::*;
use openapi::apis::authentifizierung_keyadder_schnittstellen::AuthentifizierungKeyadderSchnittstellen;
use openapi::apis::authentifizierung_keyadder_schnittstellen::*;
use openapi::models::RotationResponse;
use openapi::models::{self, AuthListingKeytag200Response};
use tracing::{Instrument, debug, error, info, instrument, warn};

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

async fn internal_extract_claims(
    server: &LTZFServer,
    headers: &axum::http::header::HeaderMap,
    key: &str,
) -> Result<crate::api::Claims> {
    let key = headers.get(key);
    if key.is_none() {
        error!("Key was None, expected to find it in the headers");
        return Err(LTZFError::Validation {
            source: Box::new(crate::error::DataValidationError::MissingField {
                field: "X-API-Key".to_string(),
            }),
        });
    }
    let key = key.unwrap().to_str()?;
    let tag = crate::utils::auth::keytag_of(key);
    debug!("Authenticating Key: `{}`", tag);

    if let Some((id, deleted_by, expiry, scope, salt, hash)) = sqlx::query!(
        "SELECT k.id, k.deleted_by, k.expires_at, value as scope, k.salt, k.key_hash
        FROM api_keys k
        INNER JOIN api_scope s ON s.id = k.scope
        WHERE keytag = $1",
        tag
    )
    .map(|r| {
        (
            r.id,
            r.deleted_by,
            r.expires_at,
            r.scope,
            r.salt.to_string(),
            r.key_hash,
        )
    })
    .fetch_optional(&server.sqlx_db)
    .await?
    {
        let incoming_hash = crate::utils::auth::hash_full_key(&salt, key);
        if hash != incoming_hash {
            warn!("Hash is not matching");
            Err(LTZFError::Validation {
                source: Box::new(crate::error::DataValidationError::Unauthorized {
                    reason: format!("API Key is not valid. Tag: {tag}"),
                }),
            })
        } else if let Some(deleted_by) = deleted_by {
            if deleted_by == id {
                warn!("API Key was valid but was either rotated or expired");
                Err(LTZFError::Validation {
                    source: Box::new(crate::error::DataValidationError::Unauthorized {
                        reason: format!(
                            "API Key was valid but was either rotated or expired. Tag: {tag}"
                        ),
                    }),
                })
            } else {
                let delkeytag = sqlx::query!("SELECT keytag FROM api_keys WHERE id=$1", deleted_by)
                    .map(|r| r.keytag)
                    .fetch_one(&server.sqlx_db)
                    .await?;
                warn!("Key was deleted by {delkeytag}");
                Err(LTZFError::Validation {
                    source: Box::new(crate::error::DataValidationError::Unauthorized {
                        reason: format!(
                            "API Key was valid but is deleted. Tag: {tag}\nAdministrator Key: {delkeytag}"
                        ),
                    }),
                })
            }
        } else if expiry < chrono::Utc::now() {
            sqlx::query!("UPDATE api_keys SET deleted_by = $1 WHERE id = $1", id)
                .execute(&server.sqlx_db)
                .await?;
            warn!("Key was valid but has expired since last use");
            Err(LTZFError::Validation {
                source: Box::new(crate::error::DataValidationError::Unauthorized {
                    reason: format!("API Key was valid but has expired. Tag: {tag}"),
                }),
            })
        } else {
            let scope = (APIScope::try_from(scope.as_str()).unwrap(), id);
            sqlx::query!(
                "UPDATE api_keys SET last_used = $1 WHERE id = $2",
                chrono::Utc::now(),
                id
            )
            .execute(&server.sqlx_db)
            .await?;
            debug!("Scope of key with tag`{}`: {:?}", tag, scope.0);
            Ok(scope)
        }
    } else {
        warn!("Key was not found in the database");
        Err(LTZFError::Validation {
            source: Box::new(crate::error::DataValidationError::Unauthorized {
                reason: "API Key was not found in the Database".to_string(),
            }),
        })
    }
}

#[async_trait]
impl ApiKeyAuthHeader for LTZFServer {
    type Claims = crate::api::Claims;

    #[instrument(skip_all)]
    async fn extract_claims_from_header(
        &self,
        headers: &axum::http::header::HeaderMap,
        key: &str,
    ) -> Option<Self::Claims> {
        let current = tracing::span::Span::current().clone();
        let result = internal_extract_claims(self, headers, key)
            .instrument(current)
            .await;
        match result {
            Ok(claim) => Some(claim),
            Err(error) => {
                warn!("Authorization failed: {}", error);
                None
            }
        }
    }
}

#[async_trait]
impl AuthentifizierungKeyadderSchnittstellen<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;

    #[instrument(skip_all, fields(claim=%claims.0))]
    async fn auth_listing(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        query_params: &models::AuthListingQueryParams,
    ) -> Result<AuthListingResponse> {
        if claims.0 != APIScope::KeyAdder {
            warn!("Permission level too low");
            return Ok(AuthListingResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let mut tx = self.sqlx_db.begin().await?;
        let total_count = sqlx::query!("SELECT COUNT(1) AS total_count FROM api_keys WHERE deleted_by IS NULL AND expires_at > NOW()")
        .map(|r| r.total_count)
        .fetch_one(&mut *tx).await?.unwrap();
        let prp = PaginationResponsePart::new(
            total_count as i32,
            query_params.page,
            query_params.per_page,
        );
        let result = sqlx::query!(
            "SELECT keytag FROM api_keys WHERE deleted_by IS NULL AND expires_at > NOW() ORDER BY expires_at DESC
            OFFSET $1
            LIMIT $2", prp.offset(), prp.limit())
        .map(|r| models::KeyTag::from(r.keytag))
        .fetch_all(&mut *tx).await?;

        tx.commit().await?;
        info!("Listing {} keys", &result.len());
        return Ok(AuthListingResponse::Status200_OK {
            body: result,
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
            x_total_count: Some(prp.x_total_count),
            x_total_pages: Some(prp.x_total_pages),
            x_page: Some(prp.x_page),
            x_per_page: Some(prp.x_per_page),
            link: Some(prp.generate_link_header("/api/v2/auth/keys")),
        });
    }

    #[instrument(skip_all, fields(claim=%claims.0))]
    async fn auth_listing_keytag(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::AuthListingKeytagPathParams,
    ) -> Result<AuthListingKeytagResponse> {
        if claims.0 != APIScope::KeyAdder {
            warn!("Permission level too low");
            return Ok(AuthListingKeytagResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let dings = sqlx::query!(
            "SELECT 1 AS disc, api_id FROM scraper_touched_vorgang tb
            INNER JOIN vorgang v ON v.id= tb.vg_id
			INNER JOIN api_keys ak ON ak.id = tb.collector_key
            WHERE ak.keytag = $1
            
            UNION ALL
            
            SELECT 2, api_id FROM scraper_touched_station tb
            INNER JOIN station s ON s.id=tb.stat_id
			INNER JOIN api_keys ak ON ak.id = tb.collector_key
            WHERE ak.keytag = $1

            UNION ALL

            SELECT 3, api_id FROM scraper_touched_sitzung tb
            INNER JOIN sitzung s ON s.id=tb.sid
			INNER JOIN api_keys ak ON ak.id = tb.collector_key
            WHERE ak.keytag = $1

            UNION ALL

            SELECT 4, api_id FROM scraper_touched_dokument tb
            INNER JOIN dokument d ON d.id = tb.dok_id
			INNER JOIN api_keys ak ON ak.id = tb.collector_key
            WHERE ak.keytag = $1",
            path_params.keytag.to_string()
        )
        .fetch_all(&self.sqlx_db)
        .await?;

        let vorgaenge = as_option(
            dings
                .iter()
                .filter(|r| r.disc == Some(1))
                .map(|x| x.api_id.unwrap().to_string())
                .collect(),
        );
        let stationen = as_option(
            dings
                .iter()
                .filter(|r| r.disc == Some(2))
                .map(|x| x.api_id.unwrap().to_string())
                .collect(),
        );
        let sitzungen = as_option(
            dings
                .iter()
                .filter(|r| r.disc == Some(3))
                .map(|x| x.api_id.unwrap().to_string())
                .collect(),
        );
        let dokumente = as_option(
            dings
                .iter()
                .filter(|r| r.disc == Some(4))
                .map(|x| x.api_id.unwrap().to_string())
                .collect(),
        );
        let result = AuthListingKeytag200Response {
            dokumente,
            vorgaenge,
            sitzungen,
            stationen,
        };
        let r = &result;
        info!(
            "Successfully fetched {}/{}/{}/{} (d/v/si/st) entries",
            r.dokumente.as_ref().map(|x| x.len()).unwrap_or(0),
            r.vorgaenge.as_ref().map(|x| x.len()).unwrap_or(0),
            r.sitzungen.as_ref().map(|x| x.len()).unwrap_or(0),
            r.stationen.as_ref().map(|x| x.len()).unwrap_or(0),
        );
        return Ok(AuthListingKeytagResponse::Status200_OK {
            body: result,
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
        });
    }

    #[doc = "AuthDelete - DELETE /api/v2/auth"]
    #[instrument(skip_all, fields(claim=%claims.0, keytag=%header_params.api_key_delete))]
    async fn auth_delete(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        header_params: &models::AuthDeleteHeaderParams,
    ) -> Result<AuthDeleteResponse> {
        if claims.0 != APIScope::KeyAdder {
            warn!("Permission level too low");
            return Ok(AuthDeleteResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        sqlx::query!(
            "UPDATE api_keys SET deleted_by=$1 WHERE keytag=$2",
            claims.1,
            header_params.api_key_delete
        )
        .execute(&self.sqlx_db)
        .await?;

        info!("Successfully deleted keys");
        Ok(AuthDeleteResponse::Status204_NoContent {
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
        })
    }

    #[doc = "AuthPost - POST /api/v2/auth"]
    #[instrument(skip_all, fields(claim=%claims.0))]
    async fn auth_post(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        body: &models::CreateApiKey,
    ) -> Result<AuthPostResponse> {
        if claims.0 != APIScope::KeyAdder {
            warn!("Permission level too low");
            return Ok(AuthPostResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let mut tx = self.sqlx_db.begin().await?;
        let (key, salt) = crate::utils::auth::find_new_key(&mut tx).await?;
        let tag = crate::utils::auth::keytag_of(&key);

        let key_digest = crate::utils::auth::hash_full_key(&salt, &key);

        sqlx::query!(
            "INSERT INTO api_keys(key_hash, created_by, expires_at, scope, salt, keytag)
        VALUES
        ($1, $2, $3, (SELECT id FROM api_scope WHERE value = $4), $5, $6)",
            key_digest,
            claims.1,
            body.expires_at
                .unwrap_or(chrono::Utc::now() + chrono::Duration::days(365)),
            body.scope.to_string(),
            salt,
            tag
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        tracing::info!(
            "Generated Fresh API Key with Scope: {} and keytag {}",
            body.scope,
            tag
        );
        Ok(AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(
            key,
        ))
    }
}

#[async_trait]
impl Authentifizierung<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;
    #[instrument(skip_all, fields(claim=%claims.0))]
    /// SECURITY:
    /// There should always ever be at most two keys in an active rotation relationship (rotated_for!=NULL && deleted_by!=NULL)
    /// And the rotation date must never be expanded, otherwise you can just prolong your old key's life indefinitely
    ///
    /// Names: old_key: The key that the request has been authenticated with
    /// new_key: the key that is created and succeeds old_key
    /// some_key: Any key in the database
    /// Cases:
    /// 1. old_key.rotated_for != None  => SET old_key.rotated_for.deleted_by=himself, old_key.rotated_for=new_key,
    /// 2. some_key.rotated_for=old_key => some_key.deleted_by=some_key, old_key.rotated_for=new_key
    #[instrument(skip_all, fields(claim=%claims.0))]
    async fn auth_rotate(
        &self,
        _method: &axum::http::Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
    ) -> Result<AuthRotateResponse> {
        let mut tx = self.sqlx_db.begin().await?;
        let old_key_entry = sqlx::query!(
            "SELECT scope,value as named_scope, expires_at, created_at, rotated_for, keytag
            FROM api_keys INNER JOIN api_scope ON scope=api_scope.id 
            WHERE api_keys.id = $1",
            claims.1
        )
        .fetch_one(&mut *tx)
        .await?;

        // new key, replacing the old one
        let (new_key, new_salt) = crate::utils::auth::find_new_key(&mut tx).await?;
        let new_hash = crate::utils::auth::hash_full_key(&new_salt, &new_key);
        let new_keytag = crate::utils::auth::keytag_of(&new_key);

        let new_id = sqlx::query!(
            "INSERT INTO api_keys(key_hash, created_by, expires_at, scope, salt, keytag)
        VALUES
        ($1, $2, $3, $4,$5, $6)
        RETURNING id",
            new_hash,
            claims.1,
            chrono::Utc::now() + (old_key_entry.expires_at - old_key_entry.created_at),
            old_key_entry.scope,
            new_salt,
            new_keytag
        )
        .map(|r| r.id)
        .fetch_one(&self.sqlx_db)
        .await?;

        // at this point, fix up some failure cases:
        // 1. if the old key is in rotation state (rotated_for is not NULL): invalidate the one it is rotated for
        if let Some(rf) = old_key_entry.rotated_for {
            debug!("#1: old key is in rotation state: Invalidate the one it is rotated for");
            sqlx::query!("UPDATE api_keys SET deleted_by = id WHERE id = $1", rf)
                .execute(&mut *tx)
                .await?;
        }

        // 2. if the old key has a corresponding rotated_for entry: invalidate that one, and handle the old one like default
        // this prohibits creating any number of successor keys in an infinite chain. That way there is at most one pair of [old key] -> [new key]
        let r = sqlx::query!("SELECT id FROM api_keys WHERE rotated_for = $1", claims.1)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(inv_id) = r {
            debug!("#2: old key has a 'fresh_key' role for a different key: Invalidate that one");
            sqlx::query!(
                "UPDATE api_keys SET deleted_by = id WHERE id = $1",
                inv_id.id
            )
            .execute(&mut *tx)
            .await?;
        }
        // set expiry to min(1d, existing) (basically: prohibit overriding the expiration date of an existing key)
        let rot_expiration_date = chrono::Utc::now() + chrono::Duration::days(1);
        sqlx::query!(
            "UPDATE api_keys 
        SET expires_at = LEAST($2::timestamptz, expires_at), rotated_for = $3
        WHERE id = $1",
            claims.1,
            rot_expiration_date.clone(),
            new_id
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        info!(
            "Rotated API Key {} with Scope: {:?}",
            old_key_entry.keytag, old_key_entry.named_scope
        );
        Ok(AuthRotateResponse::Status201_RotationSuccessful(
            RotationResponse {
                new_api_key: new_key,
                rotation_complete_date: rot_expiration_date,
            },
        ))
    }
    /// AuthStatus - GET /api/v2/auth/status
    #[instrument(skip_all, fields(claim=%claims.0))]
    async fn auth_status(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
    ) -> Result<AuthStatusResponse> {
        let db_row = sqlx::query!(
            "SELECT * FROM 
        api_keys WHERE id = $1",
            claims.1
        )
        .fetch_one(&self.sqlx_db)
        .await?;
        let rotation_is_active =
            db_row.rotated_for.is_some() && db_row.expires_at > chrono::Utc::now();
        debug!("Key {} ({}) requested auth status", db_row.keytag, claims.0);
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
    use axum_extra::extract::CookieJar;
    use openapi::apis::authentifizierung::{
        AuthRotateResponse, AuthStatusResponse, Authentifizierung,
    };
    use openapi::apis::authentifizierung_keyadder_schnittstellen::*;
    use openapi::apis::collector_schnittstellen_vorgang::CollectorSchnittstellenVorgang;
    use openapi::models::{self, AuthListingQueryParams};

    use crate::LTZFServer;
    use crate::utils::auth::keytag_of;
    use crate::utils::testing::{TestSetup, generate};

    async fn fetch_key_index(server: &LTZFServer, keytag: String) -> i32 {
        fetch_key_row(server, keytag).await.id
    }
    fn localhost() -> headers::Host {
        use http::uri::Authority;
        Authority::from_static("localhost").into()
    }
    #[allow(unused)]
    struct KeyRow {
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
            .map(|r| KeyRow {
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
                keytag: r.keytag,
            })
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        index
    }

    // GET /auth/status
    #[tokio::test]
    async fn test_auth_status() {
        let scenario = TestSetup::new("test_auth_status").await;
        let server = &scenario.server;
        let expiry_date = chrono::Utc::now() + chrono::Duration::days(2);
        let key = server
            .auth_post(
                &Method::POST,
                &localhost(),
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
                &localhost(),
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
                &localhost(),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, original_key_idx),
            )
            .await
            .unwrap();
        let fresh_key = match rotstruct {
            AuthRotateResponse::Status201_RotationSuccessful(rots) => rots.new_api_key,
            _ => unreachable!("Unreachable"),
        };
        let fresh_key_index = fetch_key_index(server, keytag_of(&fresh_key)).await;
        let fresh_key_status = fetch_key_status(server, fresh_key_index).await;
        assert!(!fresh_key_status.is_being_rotated && fresh_key_status.scope == "keyadder");

        let key_status_rot = server
            .auth_status(
                &Method::POST,
                &localhost(),
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
                &localhost(),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, index),
            )
            .await
            .unwrap()
        {
            AuthStatusResponse::Status200_SuccessfullyRetrievedAPIKeyStatus(stat) => stat,
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
                    &localhost(),
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
                    &localhost(),
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
                &localhost(),
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
                &localhost(),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1),
                &models::CreateApiKey {
                    scope: "keyadder".to_string(),
                    expires_at: None,
                },
            )
            .await
        {
            Ok(AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(key)) => keys.push(key),
            _ => unreachable!(),
        }
        match server
            .auth_post(
                &Method::POST,
                &localhost(),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1),
                &models::CreateApiKey {
                    scope: "collector".to_string(),
                    expires_at: None,
                },
            )
            .await
        {
            Ok(AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(key)) => keys.push(key),
            _ => unreachable!(),
        }
        match server
            .auth_post(
                &Method::POST,
                &localhost(),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1),
                &models::CreateApiKey {
                    scope: "keyadder".to_string(),
                    expires_at: None,
                },
            )
            .await
        {
            Ok(AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(key)) => keys.push(key),
            _ => unreachable!(),
        }
        match server
            .auth_post(
                &Method::POST,
                &localhost(),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1),
                &models::CreateApiKey {
                    scope: "admin".to_string(),
                    expires_at: None,
                },
            )
            .await
        {
            Ok(AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(key)) => keys.push(key),
            _ => unreachable!(),
        }
        // ------------------------------------------------------------------------------------------------------------
        let response = server
            .auth_listing(
                &Method::GET,
                &localhost(),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1),
                &AuthListingQueryParams {
                    page: None,
                    per_page: None,
                    since: None,
                    until: None,
                },
            )
            .await;
        match response {
            Ok(AuthListingResponse::Status200_OK { body, .. }) => {
                let mut bodykeys: Vec<_> = body
                    .iter()
                    .map(|x| {
                        x.as_str()
                            .strip_prefix("\"")
                            .unwrap_or(x.as_str())
                            .strip_suffix("\"")
                            .unwrap_or(x.as_str())
                    })
                    .collect();
                bodykeys.sort();
                let mut keys = keys
                    .iter()
                    .map(|x| keytag_of(x))
                    .chain(vec!["total-nutzlos".to_string()].drain(..))
                    .collect::<Vec<_>>();
                keys.sort();
                assert_eq!(keys, bodykeys);
            }
            _ => unreachable!("unreachable"),
        }
        // insufficient permissions
        let response = server
            .auth_listing(
                &Method::GET,
                &localhost(),
                &CookieJar::new(),
                &(super::APIScope::Admin, 1),
                &AuthListingQueryParams {
                    page: None,
                    per_page: None,
                    since: None,
                    until: None,
                },
            )
            .await;
        assert!(matches!(
            response,
            Ok(AuthListingResponse::Status403_Forbidden { .. })
        ));
        let response = server
            .auth_listing(
                &Method::GET,
                &localhost(),
                &CookieJar::new(),
                &(super::APIScope::Collector, 1),
                &AuthListingQueryParams {
                    page: None,
                    per_page: None,
                    since: None,
                    until: None,
                },
            )
            .await;
        assert!(matches!(
            response,
            Ok(AuthListingResponse::Status403_Forbidden { .. })
        ));

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
                &localhost(),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1),
                &models::CreateApiKey {
                    scope: "keyadder".to_string(),
                    expires_at: None,
                },
            )
            .await;
        let key = match rsp {
            Ok(AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(key)) => key,
            _ => unreachable!(),
        };
        let key_idx = fetch_key_index(server, keytag_of(&key)).await;

        let _ = server
            .vorgang_put(
                &Method::PUT,
                &localhost(),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, key_idx),
                &models::VorgangPutHeaderParams {
                    x_scraper_id: uuid::Uuid::nil(),
                },
                &generate::default_vorgang(),
            )
            .await
            .unwrap();
        // enough setup, here it comes:
        let rsp = server
            .auth_listing_keytag(
                &Method::PUT,
                &localhost(),
                &CookieJar::new(),
                &(super::APIScope::KeyAdder, 1),
                &models::AuthListingKeytagPathParams {
                    keytag: keytag_of(&key),
                },
            )
            .await;

        match rsp {
            Ok(AuthListingKeytagResponse::Status200_OK { body, .. }) => {
                assert!(body.dokumente.is_some() && body.dokumente.unwrap().len() == 2);
                assert!(body.sitzungen.is_none());
                assert!(body.stationen.is_some() && body.stationen.unwrap().len() == 1);
                assert!(body.vorgaenge.is_some() && body.vorgaenge.unwrap().len() == 1);
            }
            _ => unreachable!(),
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
                &localhost(),
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
                &localhost(),
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
        assert_eq!(row.deleted_by, Some(1));

        // delete already deleted key
        let rsp = server
            .auth_delete(
                &Method::POST,
                &localhost(),
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
        assert_eq!(row.deleted_by, Some(1));

        // delete without permission
        let rsp = server
            .auth_delete(
                &Method::POST,
                &localhost(),
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
                &localhost(),
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
            let rrsp = server
                .auth_rotate(
                    &Method::POST,
                    &localhost(),
                    &CookieJar::new(),
                    &(super::APIScope::KeyAdder, 2),
                )
                .await
                .unwrap();
            assert!(matches!(
                rrsp,
                AuthRotateResponse::Status201_RotationSuccessful(..)
            ));
            match rrsp {
                AuthRotateResponse::Status201_RotationSuccessful(s) => s,
                _ => unreachable!("not possible"),
            }
            // state of the db:
            // 1. superadmin key
            // 2. key in rotation (1d)
            // 3. freshly generated key (1y)
        }
        .new_api_key;
        let key1_idx = fetch_key_index(server, keytag_of(&key1)).await;

        // testing that the time limits work and a "standalone" rotation is possible
        let key0_status = fetch_key_status(server, key0_idx).await; // old key
        assert!(
            key0_status.is_being_rotated
                && key0_status.scope == "keyadder".to_string()
                && key0_status
                    .expires_at
                    .checked_sub_days(chrono::Days::new(1))
                    .unwrap()
                    <= chrono::Utc::now()
        );
        let key1_status = fetch_key_status(server, key1_idx).await; // new key
        assert!(
            !key1_status.is_being_rotated
                && key1_status.scope == "keyadder".to_string()
                && key1_status
                    .expires_at
                    .checked_sub_months(chrono::Months::new(11))
                    .unwrap()
                    >= chrono::Utc::now()
        );

        // rotate the new key and expect the key in rotation to be invalidated
        let key2 = {
            let rrsp = server
                .auth_rotate(
                    &Method::POST,
                    &localhost(),
                    &CookieJar::new(),
                    &(super::APIScope::KeyAdder, 3),
                )
                .await
                .unwrap();
            assert!(matches!(
                rrsp,
                AuthRotateResponse::Status201_RotationSuccessful(..)
            ));
            match rrsp {
                AuthRotateResponse::Status201_RotationSuccessful(s) => s,
                _ => unreachable!("not possible"),
            }
            // state of the db:
            // 1. superadmin key
            // key0: ~~key in rotation (1d)~~ -> invalidated key
            // key1: ~~freshly generated key (1y)~~ -> key in rotation (1d)
            // key2: freshly generated key (1y)
        }
        .new_api_key;
        let key2_idx = fetch_key_index(server, keytag_of(&key2)).await;
        let key2_status = fetch_key_status(server, key2_idx).await; // new key
        assert!(!key2_status.is_being_rotated);
        let key0_row = fetch_key_row(server, keytag_of(&key0)).await;
        assert_eq!(key0_row.deleted_by, Some(key0_idx));
        let key1_status = fetch_key_status(server, key1_idx).await;
        assert!(key1_status.is_being_rotated);

        // rotate the key in rotation again and expect the previously generated fresh key to be invalidated.
        // the old key's lifetime does not change from the first rotation (otherwise it could be extended -> secu risk)
        let key3 = {
            let rrsp = server
                .auth_rotate(
                    &Method::POST,
                    &localhost(),
                    &CookieJar::new(),
                    &(super::APIScope::KeyAdder, 3),
                )
                .await
                .unwrap();
            assert!(matches!(
                rrsp,
                AuthRotateResponse::Status201_RotationSuccessful(..)
            ));
            match rrsp {
                AuthRotateResponse::Status201_RotationSuccessful(s) => s,
                _ => unreachable!("not possible"),
            }
            // state of the db:
            // 1. superadmin key
            // key0: ~~key in rotation (1d)~~ -> invalidated
            // key1: ~~freshly generated key (1y)~~ -> key in rotation (original timestamp)
            // key2: ~~freshly generated key (1y)~~ -> invalidated
            // key3: freshly generated key (1y)
        }
        .new_api_key;
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
