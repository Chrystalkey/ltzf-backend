use std::sync::Arc;

use async_trait::async_trait;
use auth::APIScope;
use axum::http::Method;
use axum_extra::extract::{Host, cookie::CookieJar};

use crate::Result;
use crate::db::delete::delete_ass_by_api_id;
use crate::error::{DataValidationError, LTZFError};
use crate::utils::notify;
use crate::{Configuration, db};

use openapi::apis::default::*;
use openapi::models;

mod auth;
mod kalender;
mod objects;

#[derive(Clone)]
pub struct LTZFServer {
    pub sqlx_db: sqlx::PgPool,
    pub mailbundle: Option<Arc<notify::MailBundle>>,
    pub config: Configuration,
}
pub type LTZFArc = std::sync::Arc<LTZFServer>;
impl LTZFServer {
    pub fn new(
        sqlx_db: sqlx::PgPool,
        config: Configuration,
        mailbundle: Option<notify::MailBundle>,
    ) -> Self {
        Self {
            config,
            sqlx_db,
            mailbundle: mailbundle.map(Arc::new),
        }
    }
}

#[async_trait]
impl openapi::apis::ErrorHandler<LTZFError> for LTZFServer {
    async fn handle_error(
        &self,
        method: &axum::http::Method,
        _host: &Host,
        _cookies: &axum_extra::extract::CookieJar,
        error: LTZFError,
    ) -> std::result::Result<axum::response::Response, axum::http::StatusCode> {
        tracing::error!("An error occurred during {method} that was not expected: {error}\n");
        return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    }
}

#[allow(unused_variables)]
#[async_trait]
impl openapi::apis::default::Default<LTZFError> for LTZFServer {
    type Claims = (auth::APIScope, i32);

    #[doc = "AuthPost - POST /api/v1/auth"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn auth_post(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        body: &models::CreateApiKey,
    ) -> Result<AuthPostResponse> {
        if claims.0 != auth::APIScope::KeyAdder {
            return Ok(AuthPostResponse::Status401_APIKeyIsMissingOrInvalid);
        }
        match auth::auth_get(
            self,
            body.scope.clone().try_into().unwrap(),
            body.expires_at,
            claims.1,
        )
        .await
        {
            Ok(key) => {
                return Ok(AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(
                    key,
                ));
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    #[doc = "AuthDelete - DELETE /api/v1/auth"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn auth_delete(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        header_params: &models::AuthDeleteHeaderParams,
    ) -> Result<AuthDeleteResponse> {
        if claims.0 != APIScope::KeyAdder {
            return Ok(
                openapi::apis::default::AuthDeleteResponse::Status401_APIKeyIsMissingOrInvalid,
            );
        }
        let key_to_delete = &header_params.api_key_delete;
        return auth::auth_delete(self, key_to_delete).await;
    }

    #[doc = "KalDateGet - GET /api/v1/kalender/{parlament}/{datum}"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn kal_date_get(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        header_params: &models::KalDateGetHeaderParams,
        path_params: &models::KalDateGetPathParams,
    ) -> Result<KalDateGetResponse> {
        let mut tx = self.sqlx_db.begin().await?;
        let res =
            kalender::kal_get_by_date(path_params.datum, path_params.parlament, &mut tx, self)
                .await?;
        tx.commit().await?;
        Ok(res)
    }

    #[doc = "KalDatePut - PUT /api/v1/kalender/{parlament}/{datum}"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn kal_date_put(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::KalDatePutPathParams,
        body: &Vec<models::Sitzung>,
    ) -> Result<KalDatePutResponse> {
        let last_upd_day = chrono::Utc::now()
            .date_naive()
            .checked_sub_days(chrono::Days::new(1))
            .unwrap();
        if !(claims.0 == APIScope::Admin
            || claims.0 == APIScope::KeyAdder
            || (claims.0 == APIScope::Collector && path_params.datum > last_upd_day))
        {
            tracing::warn!(
                "Unauthorized kal_date_put with path date {} and last upd day {}",
                path_params.datum,
                last_upd_day
            );
            return Ok(KalDatePutResponse::Status401_APIKeyIsMissingOrInvalid);
        }
        let len = body.len();
        let body: Vec<_> = body
            .iter()
            .filter(|&f| f.termin.date_naive() >= last_upd_day)
            .cloned()
            .collect();

        if len != body.len() {
            tracing::info!(
                "Filtered {} Sitzung entries due to date constraints",
                len - body.len()
            );
        }

        let mut tx = self.sqlx_db.begin().await?;

        let res = kalender::kal_put_by_date(
            path_params.datum,
            path_params.parlament,
            body,
            &mut tx,
            self,
        )
        .await?;
        tx.commit().await?;
        Ok(res)
    }

    #[doc = "KalGet - GET /api/v1/kalender"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn kal_get(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        header_params: &models::KalGetHeaderParams,
        query_params: &models::KalGetQueryParams,
    ) -> Result<KalGetResponse> {
        let mut tx = self.sqlx_db.begin().await?;
        let res = kalender::kal_get_by_param(query_params, header_params, &mut tx, self).await?;
        tx.commit().await?;
        Ok(res)
    }

    #[doc = "VorgangGetById - GET /api/v1/vorgang/{vorgang_id}"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_get_by_id(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        header_params: &models::VorgangGetByIdHeaderParams,
        path_params: &models::VorgangGetByIdPathParams,
    ) -> Result<VorgangGetByIdResponse> {
        tracing::trace!(
            "vorgang_get_by_id called with id {}",
            path_params.vorgang_id
        );
        let vorgang = objects::vg_id_get(self, header_params, path_params).await;

        match vorgang {
            Ok(vorgang) => Ok(VorgangGetByIdResponse::Status200_SuccessfulOperation(
                vorgang,
            )),
            Err(e) => match &e {
                LTZFError::Validation { source } => match **source {
                    crate::error::DataValidationError::QueryParametersNotSatisfied => {
                        Ok(VorgangGetByIdResponse::Status304_NoNewChanges)
                    }
                    _ => Err(e),
                },
                LTZFError::Database { source } => match **source {
                    crate::error::DatabaseError::Sqlx {
                        source: sqlx::Error::RowNotFound,
                    } => Ok(VorgangGetByIdResponse::Status404_ContentNotFound),
                    _ => Err(e),
                },
                _ => Err(e),
            },
        }
    }
    #[doc = "VorgangDelete - DELETE /api/v1/vorgang/{vorgang_id}"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_delete(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::VorgangDeletePathParams,
    ) -> Result<VorgangDeleteResponse> {
        if claims.0 != auth::APIScope::Admin && claims.0 != auth::APIScope::KeyAdder {
            return Ok(VorgangDeleteResponse::Status401_APIKeyIsMissingOrInvalid);
        }
        db::delete::delete_vorgang_by_api_id(path_params.vorgang_id, self).await
    }

    #[doc = "VorgangIdPut - PUT /api/v1/vorgang/{vorgang_id}"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_id_put(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::VorgangIdPutPathParams,
        body: &models::Vorgang,
    ) -> Result<VorgangIdPutResponse> {
        if claims.0 != auth::APIScope::Admin && claims.0 != auth::APIScope::KeyAdder {
            return Ok(VorgangIdPutResponse::Status401_APIKeyIsMissingOrInvalid);
        }
        let out = objects::vorgang_id_put(self, path_params, body).await?;
        Ok(out)
    }

    #[doc = "VorgangGet - GET /api/v1/vorgang"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_get(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        header_params: &models::VorgangGetHeaderParams,
        query_params: &models::VorgangGetQueryParams,
    ) -> Result<VorgangGetResponse> {
        let mut tx = self.sqlx_db.begin().await?;
        match objects::vg_get(header_params, query_params, &mut tx).await {
            Ok(x) => {
                tx.commit().await?;
                Ok(x)
            }
            Err(e) => Err(e),
        }
    }

    #[doc = "VorgangPut - PUT /api/v1/vorgang"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_put(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        query_params: &models::VorgangPutQueryParams,
        body: &models::Vorgang,
    ) -> Result<VorgangPutResponse> {
        // technically not necessary since all authenticated scopes are allowed, still, better be explicit about that
        if claims.0 != APIScope::KeyAdder
            && claims.0 != APIScope::Admin
            && claims.0 != APIScope::Collector
        {
            return Ok(VorgangPutResponse::Status401_APIKeyIsMissingOrInvalid);
        }
        let rval = objects::vorgang_put(self, body).await;
        match rval {
            Ok(_) => Ok(VorgangPutResponse::Status201_Success),
            Err(e) => match &e {
                LTZFError::Validation { source } => match **source {
                    DataValidationError::AmbiguousMatch { .. } => {
                        Ok(VorgangPutResponse::Status409_Conflict)
                    }
                    _ => Err(e),
                },
                _ => Err(e),
            },
        }
    }

    #[doc = "SitzungDelete - DELETE /api/v1/sitzung/{sid}"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn sitzung_delete(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::SitzungDeletePathParams,
    ) -> Result<SitzungDeleteResponse> {
        if claims.0 != auth::APIScope::Admin && claims.0 != auth::APIScope::KeyAdder {
            return Ok(SitzungDeleteResponse::Status401_APIKeyIsMissingOrInvalid);
        }
        Ok(delete_ass_by_api_id(path_params.sid, self).await?)
    }

    #[doc = "SGetById - GET /api/v1/sitzung/{sid}"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn s_get_by_id(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        header_params: &models::SGetByIdHeaderParams,
        path_params: &models::SGetByIdPathParams,
    ) -> Result<SGetByIdResponse> {
        let ass = objects::s_get_by_id(self, header_params, path_params).await?;
        return Ok(ass);
    }

    #[doc = "SidPut - PUT /api/v1/sitzung/{sid}"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn sid_put(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::SidPutPathParams,
        body: &models::Sitzung,
    ) -> Result<SidPutResponse> {
        if claims.0 != auth::APIScope::Admin && claims.0 != auth::APIScope::KeyAdder {
            return Ok(SidPutResponse::Status401_APIKeyIsMissingOrInvalid);
        }
        let out = objects::s_id_put(self, path_params, body).await?;
        Ok(out)
    }

    #[doc = "SGet - GET /api/v1/sitzung"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn s_get(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        header_params: &models::SGetHeaderParams,
        query_params: &models::SGetQueryParams,
    ) -> Result<SGetResponse> {
        let res = objects::s_get(self, query_params, header_params).await?;
        Ok(res)
    }
}

#[cfg(test)]
mod endpoint_test {
    use super::*;
    use crate::{LTZFServer, Result};
    use axum_extra::extract::Host;
    use chrono::Utc;
    use openapi::models;
    use sha256::digest;
    use uuid::Uuid;
    const MASTER_URL: &str = "postgres://ltzf-user:ltzf-pass@localhost:5432/ltzf";

    async fn setup_server(dbname: &str) -> Result<LTZFServer> {
        let create_pool = sqlx::PgPool::connect(MASTER_URL).await.unwrap();
        sqlx::query(&format!("DROP DATABASE IF EXISTS {} WITH (FORCE);", dbname))
            .execute(&create_pool)
            .await?;
        sqlx::query(&format!(
            "CREATE DATABASE {} WITH OWNER 'ltzf-user'",
            dbname
        ))
        .execute(&create_pool)
        .await?;
        let pool = sqlx::PgPool::connect(&format!(
            "postgres://ltzf-user:ltzf-pass@localhost:5432/{}",
            dbname
        ))
        .await
        .unwrap();
        sqlx::migrate!().run(&pool).await?;
        let hash = digest("total-nutzloser-wert");
        sqlx::query!(
            "INSERT INTO api_keys(key_hash, scope, created_by)
            VALUES
            ($1, (SELECT id FROM api_scope WHERE value = 'keyadder' LIMIT 1), (SELECT last_value FROM api_keys_id_seq))
            ON CONFLICT DO NOTHING;", hash)
        .execute(&pool).await?;
        Ok(LTZFServer::new(pool, Configuration::default(), None))
    }
    async fn cleanup_server(dbname: &str) -> Result<()> {
        let create_pool = sqlx::PgPool::connect(MASTER_URL).await.unwrap();
        sqlx::query(&format!("DROP DATABASE {} WITH (FORCE);", dbname))
            .execute(&create_pool)
            .await?;
        Ok(())
    }

    // Authentication tests
    #[tokio::test]
    async fn test_auth_auth() {
        let server = setup_server("test_auth").await.unwrap();
        let resp = server
            .auth_post(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(auth::APIScope::Collector, 1),
                &models::CreateApiKey {
                    scope: "admin".to_string(),
                    expires_at: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(resp, AuthPostResponse::Status401_APIKeyIsMissingOrInvalid);

        let resp = server
            .auth_post(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(auth::APIScope::Admin, 1),
                &models::CreateApiKey {
                    scope: "collector".to_string(),
                    expires_at: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(resp, AuthPostResponse::Status401_APIKeyIsMissingOrInvalid);

        let resp = server
            .auth_post(
                &Method::POST,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(auth::APIScope::KeyAdder, 1),
                &models::CreateApiKey {
                    scope: "keyadder".to_string(),
                    expires_at: None,
                },
            )
            .await
            .unwrap();
        assert_ne!(resp, AuthPostResponse::Status401_APIKeyIsMissingOrInvalid);
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
                &(auth::APIScope::Collector, 1),
                &models::AuthDeleteHeaderParams {
                    api_key_delete: key.clone(),
                },
            )
            .await
            .unwrap();
        assert_eq!(del, AuthDeleteResponse::Status401_APIKeyIsMissingOrInvalid);
        let del = server
            .auth_delete(
                &Method::DELETE,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(auth::APIScope::Admin, 1),
                &models::AuthDeleteHeaderParams {
                    api_key_delete: key.clone(),
                },
            )
            .await
            .unwrap();
        assert_eq!(del, AuthDeleteResponse::Status401_APIKeyIsMissingOrInvalid);

        let del = server
            .auth_delete(
                &Method::DELETE,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(auth::APIScope::KeyAdder, 1),
                &models::AuthDeleteHeaderParams {
                    api_key_delete: "unknown-keyhash".to_string(),
                },
            )
            .await
            .unwrap();
        assert_eq!(del, AuthDeleteResponse::Status404_APIKeyNotFound);

        let del = server
            .auth_delete(
                &Method::DELETE,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(auth::APIScope::KeyAdder, 1),
                &models::AuthDeleteHeaderParams {
                    api_key_delete: key,
                },
            )
            .await
            .unwrap();
        assert_eq!(del, AuthDeleteResponse::Status204_Success);

        cleanup_server("test_auth").await.unwrap();
    }

    // Calendar tests
    #[tokio::test]
    async fn test_calendar_endpoints() {
        // Test cases for kal_date_put:
        // - Update calendar entry with valid data
        // - Update calendar entry with insufficient permissions
        // - Update calendar entry with date constraints

        // Test cases for kal_date_get:
        // - Get calendar entry for valid date and parliament
        // - Get calendar entry for non-existent date

        // Test cases for kal_get:
        // - Get calendar entries with valid parameters
        // - Get calendar entries with invalid parameters
        // - Get calendar entries with date range
    }

    // Procedure (Vorgang) tests
    #[tokio::test]
    async fn test_vorgang_get_endpoints() {
        // Setup test server and database
        let server = setup_server("test_vorgang_get").await.unwrap();

        // Test cases for vorgang_get_by_id:
        // 1. Get existing procedure
        {
            let test_vorgang = create_test_vorgang();
            // First create the procedure
            let create_response = server
                .vorgang_put(
                    &Method::PUT,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(auth::APIScope::Collector, 1),
                    &models::VorgangPutQueryParams {
                        collector: test_vorgang.api_id,
                    },
                    &test_vorgang,
                )
                .await
                .unwrap();
            assert_eq!(create_response, VorgangPutResponse::Status201_Success);

            // Then get it by ID
            let response = server
                .vorgang_get_by_id(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::VorgangGetByIdHeaderParams {
                        if_modified_since: None,
                    },
                    &models::VorgangGetByIdPathParams {
                        vorgang_id: test_vorgang.api_id,
                    },
                )
                .await
                .unwrap();
            match response {
                VorgangGetByIdResponse::Status200_SuccessfulOperation(vorgang) => {
                    assert_eq!(vorgang.api_id, test_vorgang.api_id);
                    assert_eq!(vorgang.titel, test_vorgang.titel);
                }
                _ => panic!("Expected successful operation response"),
            }
        }

        // 2. Get non-existent procedure
        {
            let non_existent_id = Uuid::now_v7();
            let response = server
                .vorgang_get_by_id(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::VorgangGetByIdHeaderParams {
                        if_modified_since: None,
                    },
                    &models::VorgangGetByIdPathParams {
                        vorgang_id: non_existent_id,
                    },
                )
                .await
                .unwrap();
            assert_eq!(response, VorgangGetByIdResponse::Status404_ContentNotFound);
        }

        // 3. Get procedure with invalid ID
        {
            let invalid_id = Uuid::nil();
            let response = server
                .vorgang_get_by_id(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::VorgangGetByIdHeaderParams {
                        if_modified_since: None,
                    },
                    &models::VorgangGetByIdPathParams {
                        vorgang_id: invalid_id,
                    },
                )
                .await
                .unwrap();
            assert_eq!(response, VorgangGetByIdResponse::Status404_ContentNotFound);
        }

        // Test cases for vorgang_get:
        // 1. Get procedures with valid parameters
        {
            let response = server
                .vorgang_get(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::VorgangGetHeaderParams {
                        if_modified_since: None,
                    },
                    &models::VorgangGetQueryParams {
                        limit: Some(10),
                        offset: Some(0),
                        p: None,
                        since: None,
                        until: None,
                        vgtyp: None,
                        wp: None,
                        inifch: None,
                        iniorg: None,
                        inipsn: None,
                    },
                )
                .await
                .unwrap();
            match response {
                VorgangGetResponse::Status200_AntwortAufEineGefilterteAnfrageZuVorgang(vorgange) => {
                    assert!(!vorgange.is_empty());
                }
                _ => panic!("Expected successful operation response"),
            }
        }

        // 2. Get procedures with invalid parameters
        {
            let response = server
                .vorgang_get(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::VorgangGetHeaderParams {
                        if_modified_since: None,
                    },
                    &models::VorgangGetQueryParams {
                        limit: None, // Invalid limit
                        offset: None, // Invalid offset
                        p: None,
                        since: Some(Utc::now()),
                        until: Some(Utc::now() - chrono::Duration::days(365)),
                        vgtyp: None,
                        wp: None,
                        inifch: None,
                        iniorg: None,
                        inipsn: None,
                    },
                )
                .await
                .unwrap();
            assert_eq!(response, VorgangGetResponse::Status416_RequestRangeNotSatisfiable);
            let response = server
                .vorgang_get(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::VorgangGetHeaderParams {
                        if_modified_since: None,
                    },
                    &models::VorgangGetQueryParams {
                        limit: None, // Invalid limit
                        offset: None, // Invalid offset
                        p: None,
                        since: Some(Utc::now() + chrono::Duration::days(365)),
                        until: Some(Utc::now() + chrono::Duration::days(366)),
                        vgtyp: None,
                        wp: None,
                        inifch: None,
                        iniorg: None,
                        inipsn: None,
                    },
                )
                .await
                .unwrap();
            assert_eq!(response, VorgangGetResponse::Status204_NoContentFoundForTheSpecifiedParameters);
        }

        // 3. Get procedures with filters
        {
            let test_vorgang = create_test_vorgang();
            // First create a procedure with specific parameters
            let create_response = server
                .vorgang_put(
                    &Method::PUT,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(auth::APIScope::Collector, 1),
                    &models::VorgangPutQueryParams {
                        collector: test_vorgang.api_id,
                    },
                    &test_vorgang,
                )
                .await
                .unwrap();
            assert_eq!(create_response, VorgangPutResponse::Status201_Success);

            // Then get it with matching filters
            let response = server
                .vorgang_get(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::VorgangGetHeaderParams {
                        if_modified_since: None,
                    },
                    &models::VorgangGetQueryParams {
                        limit: Some(10),
                        offset: Some(0),
                        p: Some(models::Parlament::Bt),
                        since: None,
                        until: None,
                        vgtyp: Some(test_vorgang.typ),
                        wp: Some(test_vorgang.wahlperiode as i32),
                        inifch: None,
                        iniorg: None,
                        inipsn: None,
                    },
                )
                .await
                .unwrap();
            match response {
                VorgangGetResponse::Status200_AntwortAufEineGefilterteAnfrageZuVorgang(vorgange) => {
                    assert!(!vorgange.is_empty());
                }
                _ => panic!("Expected successful operation response"),
            }
        }

        // Cleanup
        cleanup_server("test_vorgang_get").await.unwrap();
    }

    #[tokio::test]
    async fn test_vorgang_put_endpoint() {
        // Setup test server and database
        let server = setup_server("test_vorgang_put").await.unwrap();

        // Test cases for vorgang_id_put:
        // 1. Update existing procedure with valid data and admin permissions
        {
            let test_vorgang = create_test_vorgang();
            let response = server
                .vorgang_id_put(
                    &Method::PUT,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(auth::APIScope::Admin, 1),
                    &models::VorgangIdPutPathParams {
                        vorgang_id: test_vorgang.api_id,
                    },
                    &test_vorgang,
                )
                .await
                .unwrap();
            assert_eq!(response, VorgangIdPutResponse::Status201_Created);
        }

        // 2. Update procedure with insufficient permissions (Collector)
        {
            let test_vorgang = create_test_vorgang();
            let response = server
                .vorgang_id_put(
                    &Method::PUT,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(auth::APIScope::Collector, 1),
                    &models::VorgangIdPutPathParams {
                        vorgang_id: test_vorgang.api_id,
                    },
                    &test_vorgang,
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                VorgangIdPutResponse::Status401_APIKeyIsMissingOrInvalid
            );
        }

        // Test cases for vorgang_put:
        // 1. Create new procedure with valid data and collector permissions
        {
            let test_vorgang = create_test_vorgang();
            let response = server
                .vorgang_put(
                    &Method::PUT,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(auth::APIScope::Collector, 1),
                    &models::VorgangPutQueryParams {
                        collector: test_vorgang.api_id,
                    },
                    &test_vorgang,
                )
                .await
                .unwrap();
            assert_eq!(response, VorgangPutResponse::Status201_Success);
        }

        // 2. Handle ambiguous matches (conflict)
        {
            // TODO
        }

        // Cleanup
        cleanup_server("test_vorgang_put").await.unwrap();
    }

    #[tokio::test]
    async fn test_vorgang_delete_endpoints() {
        // Setup test server and database
        let server = setup_server("test_vorgang_delete").await.unwrap();
        // Test cases for vorgang_delete:
        // 1. Delete existing procedure with proper permissions
        {
            let test_vorgang = create_test_vorgang();
            // First create the procedure
            let create_response = server
                .vorgang_put(
                    &Method::PUT,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(auth::APIScope::Collector, 1),
                    &models::VorgangPutQueryParams {
                        collector: Uuid::now_v7(),
                    },
                    &test_vorgang,
                )
                .await
                .unwrap();
            assert_eq!(create_response, VorgangPutResponse::Status201_Success);

            // Then delete it
            let response = server
                .vorgang_delete(
                    &Method::DELETE,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(auth::APIScope::Admin, 1),
                    &models::VorgangDeletePathParams {
                        vorgang_id: test_vorgang.api_id,
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                VorgangDeleteResponse::Status204_DeletedSuccessfully,
                "Failed to delete procedure with id {}",
                test_vorgang.api_id
            );
        }

        // 2. Delete non-existent procedure
        {
            let non_existent_id = Uuid::now_v7();
            let response = server
                .vorgang_delete(
                    &Method::DELETE,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(auth::APIScope::Admin, 1),
                    &models::VorgangDeletePathParams {
                        vorgang_id: non_existent_id,
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                VorgangDeleteResponse::Status404_NoElementWithThisID
            );
        }

        // 3. Delete procedure with insufficient permissions
        {
            let test_vorgang = create_test_vorgang();
            let response = server
                .vorgang_delete(
                    &Method::DELETE,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(auth::APIScope::Collector, 1),
                    &models::VorgangDeletePathParams {
                        vorgang_id: test_vorgang.api_id,
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                VorgangDeleteResponse::Status401_APIKeyIsMissingOrInvalid
            );
        }

        // Cleanup
        cleanup_server("test_vorgang_delete").await.unwrap();
    }

    // Session (Sitzung) tests
    #[tokio::test]
    async fn test_session_get_endpoints() {
        // Test cases for s_get_by_id:
        // - Get existing session
        // - Get non-existent session
        // - Get session with invalid ID

        // Test cases for s_get:
        // - Get sessions with valid parameters
        // - Get sessions with invalid parameters
        // - Get sessions with filters
    }

    #[tokio::test]
    async fn test_session_modify_endpoints() {
        // Test cases for sid_put:
        // - Update existing session with valid data
        // - Update session with insufficient permissions

        // Test cases for sitzung_delete:
        // - Delete existing session with proper permissions
        // - Delete non-existent session
        // - Delete session with insufficient permissions
    }

    fn create_test_vorgang() -> models::Vorgang {
        use chrono::{DateTime, Utc};
        use openapi::models::{
            Autor, DokRef, Doktyp, Dokument, Parlament, Station, Stationstyp, VgIdent, VgIdentTyp,
            Vorgang, Vorgangstyp,
        };
        use uuid::Uuid;

        // Create a test document
        let test_doc = Dokument {
            api_id: Some(Uuid::now_v7()),
            titel: "Test Document".to_string(),
            kurztitel: None,
            vorwort: Some("Test Vorwort".to_string()),
            volltext: "Test Volltext".to_string(),
            zusammenfassung: None,
            typ: Doktyp::Entwurf,
            link: "http://example.com/doc".to_string(),
            hash: "testhash".to_string(),
            zp_modifiziert: DateTime::from(Utc::now()),
            drucksnr: None,
            zp_referenz: DateTime::from(Utc::now()),
            zp_erstellt: Some(DateTime::from(Utc::now())),
            meinung: None,
            schlagworte: None,
            autoren: vec![models::Autor {
                person: Some("Test Person".to_string()),
                organisation: "Test Organization".to_string(),
                fachgebiet: Some("Test Fachgebiet".to_string()),
                lobbyregister: None,
            }],
        };

        // Create a test station
        let test_station = Station {
            typ: Stationstyp::ParlInitiativ,
            dokumente: vec![DokRef::Dokument(Box::new(test_doc))],
            zp_start: DateTime::from(Utc::now()),
            api_id: Some(Uuid::now_v7()),
            titel: Some("Test Station".to_string()),
            gremium_federf: None,
            link: Some("http://example.com".to_string()),
            trojanergefahr: None,
            zp_modifiziert: Some(DateTime::from(Utc::now())),
            parlament: Parlament::Bt,
            gremium: None,
            schlagworte: None,
            additional_links: None,
            stellungnahmen: None,
        };

        // Create a test initiator
        let test_initiator = Autor {
            person: Some("Test Person".to_string()),
            organisation: "Test Organization".to_string(),
            fachgebiet: Some("Test Fachgebiet".to_string()),
            lobbyregister: None,
        };

        // Create a test identifier
        let test_id = VgIdent {
            id: "test-id".to_string(),
            typ: VgIdentTyp::Initdrucks,
        };

        // Create and return the test Vorgang
        Vorgang {
            api_id: Uuid::now_v7(),
            titel: "Test Vorgang".to_string(),
            kurztitel: Some("Test".to_string()),
            wahlperiode: 20,
            verfassungsaendernd: false,
            typ: Vorgangstyp::GgEinspruch,
            initiatoren: vec![test_initiator],
            ids: Some(vec![test_id]),
            links: Some(vec!["http://example.com".to_string()]),
            stationen: vec![test_station],
        }
    }
}
