use std::sync::Arc;

use async_trait::async_trait;
use axum_extra::extract::Host;

use crate::Result;
use crate::error::LTZFError;
use crate::utils::notify;
use crate::{Configuration, db};

mod auth;
mod compare;
mod misc;
mod sitzung;
mod vorgang;

pub type Claims = (auth::APIScope, i32);

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

use openapi::apis::unauthorisiert::*;
#[async_trait]
impl Unauthorisiert<LTZFError> for LTZFServer {
    async fn ping(
        &self,
        _method: &axum::http::Method,
        _host: &Host,
        _cookies: &axum_extra::extract::CookieJar,
    ) -> Result<PingResponse> {
        todo!()
    }
    async fn status(
        &self,
        _method: &axum::http::Method,
        _host: &Host,
        _cookies: &axum_extra::extract::CookieJar,
    ) -> Result<StatusResponse> {
        todo!()
    }
}

#[cfg(test)]
mod endpoint_test {
    use super::*;
    use crate::{LTZFServer, Result};
    use axum_extra::extract::Host;
    use chrono::Utc;
    use openapi::models::{self, VorgangIdPutPathParams};
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
        // Setup test server and database
        let server = setup_server("test_calendar").await.unwrap();
        let host = Host("localhost".to_string());
        let cookies = CookieJar::new();

        // Create test calendar entry
        let test_date = chrono::Utc::now().date_naive();
        let recent_date = test_date; // Define recent_date at the same scope level
        let test_session = create_test_session();
        let test_sessions = vec![test_session.clone()];

        // Test cases for kal_date_put:
        // 1. Update calendar entry with valid data and proper permissions
        {
            let response = server
                .kal_date_put(
                    &Method::PUT,
                    &host,
                    &cookies,
                    &(auth::APIScope::Admin, 1),
                    &models::KalDatePutPathParams {
                        datum: test_date,
                        parlament: models::Parlament::Bt,
                    },
                    &test_sessions,
                )
                .await
                .unwrap();
            assert_eq!(response, KalDatePutResponse::Status201_Created);
            // Allow time for database operations to complete
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }

        // 2. Update calendar entry with insufficient permissions
        {
            let response = server
                .kal_date_put(
                    &Method::PUT,
                    &host,
                    &cookies,
                    &(auth::APIScope::Collector, 1), // Using Collector scope with old date should fail
                    &models::KalDatePutPathParams {
                        datum: test_date.checked_sub_days(chrono::Days::new(5)).unwrap(), // Date more than 1 day old
                        parlament: models::Parlament::Bt,
                    },
                    &test_sessions,
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                KalDatePutResponse::Status401_APIKeyIsMissingOrInvalid
            );
        }

        // 3. Update calendar entry with date constraints (collector is allowed to update recent dates)
        {
            // Use the already defined recent_date variable instead of redefining it
            let response = server
                .kal_date_put(
                    &Method::PUT,
                    &host,
                    &cookies,
                    &(auth::APIScope::Collector, 1),
                    &models::KalDatePutPathParams {
                        datum: recent_date,
                        parlament: models::Parlament::Bt,
                    },
                    &test_sessions,
                )
                .await
                .unwrap();
            assert_eq!(response, KalDatePutResponse::Status201_Created);
            // Allow time for database operations to complete
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }

        // Test cases for kal_date_get:
        // 1. Get calendar entry for valid date and parliament
        {
            let response = server
                .kal_date_get(
                    &Method::GET,
                    &host,
                    &cookies,
                    &models::KalDateGetHeaderParams {
                        if_modified_since: None,
                    },
                    &models::KalDateGetPathParams {
                        datum: recent_date,
                        parlament: models::Parlament::Bt,
                    },
                )
                .await
                .unwrap();
            match response {
                KalDateGetResponse::Status200_AntwortAufEineGefilterteAnfrageZuSitzungen(
                    sessions,
                ) => {
                    assert!(
                        !sessions.is_empty(),
                        "Expected to find at least one session"
                    );
                    assert_eq!(sessions[0].gremium.parlament, models::Parlament::Bt);
                    assert_eq!(
                        sessions[0].termin.date_naive(),
                        recent_date,
                        "Expected to find a session with the requested date"
                    );
                }
                _ => panic!("Expected to find sessions for the valid date"),
            }
        }

        // 2. Get calendar entry for non-existent date
        {
            let non_existent_date = chrono::NaiveDate::from_ymd_opt(1900, 1, 1).unwrap();
            let response = server
                .kal_date_get(
                    &Method::GET,
                    &host,
                    &cookies,
                    &models::KalDateGetHeaderParams {
                        if_modified_since: None,
                    },
                    &models::KalDateGetPathParams {
                        datum: non_existent_date,
                        parlament: models::Parlament::Bt,
                    },
                )
                .await
                .unwrap();
            assert_eq!(response, KalDateGetResponse::Status404_NotFound);
        }
        // TODO: Test for Status304_NotModified with set If-Modified-Since Header

        // Test cases for kal_get:
        // 1. Get calendar entries with valid parameters
        {
            let response = server
                .kal_get(
                    &Method::GET,
                    &host,
                    &cookies,
                    &models::KalGetHeaderParams {
                        if_modified_since: None,
                    },
                    &models::KalGetQueryParams {
                        y: Some(recent_date.format("%Y").to_string().parse::<i32>().unwrap()),
                        m: Some(recent_date.format("%m").to_string().parse::<i32>().unwrap()),
                        dom: None,
                        gr: None,
                        limit: Some(10),
                        offset: Some(0),
                        p: Some(models::Parlament::Bt),
                        since: None,
                        until: None,
                        wp: Some(20),
                    },
                )
                .await
                .unwrap();
            match response {
                KalGetResponse::Status200_AntwortAufEineGefilterteAnfrageZuSitzungen(sessions) => {
                    assert!(
                        !sessions.is_empty(),
                        "Expected to find sessions with valid filters"
                    );
                }
                _ => panic!("Expected to find sessions with valid filters"),
            }
        }

        // 2. Get calendar entries with invalid parameters (since > until)
        {
            let response = server
                .kal_get(
                    &Method::GET,
                    &host,
                    &cookies,
                    &models::KalGetHeaderParams {
                        if_modified_since: None,
                    },
                    &models::KalGetQueryParams {
                        y: None,
                        m: None,
                        dom: None,
                        gr: None,
                        limit: None,
                        offset: None,
                        p: None,
                        since: Some(chrono::Utc::now()),
                        until: Some(chrono::Utc::now() - chrono::Duration::days(1)), // until is before since
                        wp: None,
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                KalGetResponse::Status416_RequestRangeNotSatisfiable
            );
        }

        // 3. Get calendar entries with date range
        {
            let start_date = recent_date
                .and_time(chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap())
                .and_utc();
            let end_date = recent_date
                .checked_add_days(chrono::Days::new(5))
                .unwrap()
                .and_time(chrono::NaiveTime::from_hms_opt(23, 59, 59).unwrap())
                .and_utc();

            let response = server
                .kal_get(
                    &Method::GET,
                    &host,
                    &cookies,
                    &models::KalGetHeaderParams {
                        if_modified_since: None,
                    },
                    &models::KalGetQueryParams {
                        y: None,
                        m: None,
                        dom: None,
                        gr: None,
                        limit: None,
                        offset: None,
                        p: Some(models::Parlament::Bt),
                        since: Some(start_date),
                        until: Some(end_date),
                        wp: None,
                    },
                )
                .await
                .unwrap();
            match response {
                KalGetResponse::Status200_AntwortAufEineGefilterteAnfrageZuSitzungen(sessions) => {
                    assert!(
                        !sessions.is_empty(),
                        "Expected to find sessions in date range"
                    );
                    for session in sessions {
                        assert!(
                            session.termin >= start_date && session.termin <= end_date,
                            "Found session outside requested date range"
                        );
                    }
                }
                _ => panic!("Expected to find sessions in date range"),
            }
        }

        // TODO: Test for Status304_NotModified with set If-Modified-Since Header
        // TODO: Test for Status204_NoContent

        // Cleanup
        cleanup_server("test_calendar").await.unwrap();
    }

    // Procedure (Vorgang) tests
    #[tokio::test]
    async fn test_vorgang_get_by_id_endpoints() {
        // Setup test server and database
        let server = setup_server("test_vorgang_by_id_get").await.unwrap();

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
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        // Test cases for vorgang_get_by_id:
        // 1. Get existing procedure
        {
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
        let response = server
            .vorgang_get_by_id(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &models::VorgangGetByIdHeaderParams {
                    if_modified_since: Some(chrono::Utc::now()),
                },
                &models::VorgangGetByIdPathParams {
                    vorgang_id: test_vorgang.api_id,
                },
            )
            .await
            .unwrap();
        assert_eq!(response, VorgangGetByIdResponse::Status304_NoNewChanges);
        cleanup_server("test_vorgang_by_id_get").await.unwrap();
    }

    #[tokio::test]
    async fn test_vorgang_get_filtered_endpoints() {
        let server = setup_server("test_vorgang_get_filtered").await.unwrap();
        let test_vorgang = create_test_vorgang();
        // First create the procedure
        {
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
            assert_eq!(
                create_response,
                VorgangPutResponse::Status201_Success,
                "Failed to create test procedure"
            );
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
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
                        limit: None,
                        offset: None,
                        p: None,
                        since: Some(Utc::now()),
                        until: Some(Utc::now() - chrono::Duration::days(365)), // invalid: until is before since
                        vgtyp: None,
                        wp: None,
                        inifch: None,
                        iniorg: None,
                        inipsn: None,
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                VorgangGetResponse::Status416_RequestRangeNotSatisfiable
            );
            let response = server
                .vorgang_get(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::VorgangGetHeaderParams {
                        if_modified_since: None,
                    },
                    &models::VorgangGetQueryParams {
                        limit: None,  // Invalid limit
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
            assert_eq!(
                response,
                VorgangGetResponse::Status204_NoContentFoundForTheSpecifiedParameters
            );
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
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

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
                VorgangGetResponse::Status200_AntwortAufEineGefilterteAnfrageZuVorgang(
                    vorgange,
                ) => {
                    assert!(!vorgange.is_empty());
                }
                response => panic!("Expected successful operation response, got {:?}", response),
            }
        }

        // Cleanup
        cleanup_server("test_vorgang_get_filtered").await.unwrap();
    }

    #[tokio::test]
    async fn test_vorgang_put_endpoint() {
        // Setup test server and database
        let server = setup_server("test_vorgang_put").await.unwrap();
        let host = Host("localhost".to_string());
        let cookies = CookieJar::new();

        // Test cases for vorgang_id_put:
        // 1. Update existing procedure with valid data and admin permissions
        {
            let test_vorgang = create_test_vorgang();
            let response = server
                .vorgang_id_put(
                    &Method::PUT,
                    &host,
                    &cookies,
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
                    &host,
                    &cookies,
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
                    &host,
                    &cookies,
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
            let vg1 = create_test_vorgang();
            let mut vg2 = vg1.clone();
            let mut vg3 = vg1.clone();
            vg2.api_id = Uuid::now_v7();
            vg3.api_id = Uuid::now_v7();

            let rsp1 = server
                .vorgang_id_put(
                    &Method::PUT,
                    &host,
                    &cookies,
                    &(APIScope::Admin, 1),
                    &VorgangIdPutPathParams {
                        vorgang_id: vg1.api_id,
                    },
                    &vg1,
                )
                .await
                .unwrap();
            assert_eq!(rsp1, VorgangIdPutResponse::Status201_Created);

            let rsp2 = server
                .vorgang_id_put(
                    &Method::PUT,
                    &host,
                    &cookies,
                    &(APIScope::Admin, 1),
                    &VorgangIdPutPathParams {
                        vorgang_id: vg2.api_id,
                    },
                    &vg2,
                )
                .await
                .unwrap();
            assert_eq!(rsp2, VorgangIdPutResponse::Status201_Created);

            let conflict_resp = server
                .vorgang_put(
                    &Method::PUT,
                    &host,
                    &cookies,
                    &(APIScope::Admin, 1),
                    &VorgangPutQueryParams {
                        collector: Uuid::nil(),
                    },
                    &vg3,
                )
                .await
                .unwrap();
            assert_eq!(conflict_resp, VorgangPutResponse::Status409_Conflict);
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
    fn create_test_session() -> models::Sitzung {
        use chrono::{DateTime, Utc};
        use openapi::models::{Autor, DokRef, Dokument, Gremium, Parlament, Sitzung, Top};
        use uuid::Uuid;

        // Create a test document
        let test_doc = Dokument {
            api_id: Some(Uuid::now_v7()),
            titel: "Test Document".to_string(),
            kurztitel: None,
            vorwort: Some("Test Vorwort".to_string()),
            volltext: "Test Volltext".to_string(),
            zusammenfassung: None,
            typ: openapi::models::Doktyp::Entwurf,
            link: "http://example.com/doc".to_string(),
            hash: "testhash".to_string(),
            zp_modifiziert: DateTime::from(Utc::now()),
            drucksnr: None,
            zp_referenz: DateTime::from(Utc::now()),
            zp_erstellt: Some(DateTime::from(Utc::now())),
            meinung: None,
            schlagworte: None,
            autoren: vec![Autor {
                person: Some("Test Person".to_string()),
                organisation: "Test Organization".to_string(),
                fachgebiet: Some("Test Fachgebiet".to_string()),
                lobbyregister: None,
            }],
        };

        // Create a test top
        let test_top = Top {
            titel: "Test Top".to_string(),
            dokumente: Some(vec![DokRef::Dokument(Box::new(test_doc))]),
            nummer: 1,
            vorgang_id: None,
        };

        // Create a test expert
        let test_expert = Autor {
            person: Some("Test Expert".to_string()),
            organisation: "Test Expert Organization".to_string(),
            fachgebiet: Some("Test Expert Fachgebiet".to_string()),
            lobbyregister: None,
        };

        // Create and return the test Sitzung
        Sitzung {
            api_id: Some(Uuid::now_v7()),
            nummer: 1,
            titel: Some("Test Sitzung".to_string()),
            public: true,
            termin: DateTime::from(Utc::now()),
            gremium: Gremium {
                name: "Test Gremium".to_string(),
                link: Some("http://example.com/gremium".to_string()),
                wahlperiode: 20,
                parlament: Parlament::Bt,
            },
            tops: vec![test_top],
            link: Some("http://example.com/sitzung".to_string()),
            experten: Some(vec![test_expert]),
            dokumente: None,
        }
    }

    #[tokio::test]
    async fn test_session_get_endpoints() {
        // Setup test server and database
        let server = setup_server("test_session_get").await.unwrap();

        let test_session = create_test_session();
        // First create the session
        let create_response = server
            .sid_put(
                &Method::PUT,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(auth::APIScope::Admin, 1),
                &models::SidPutPathParams {
                    sid: test_session.api_id.unwrap(),
                },
                &test_session,
            )
            .await
            .unwrap();
        assert_eq!(create_response, SidPutResponse::Status201_Created);
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        // Test cases for s_get_by_id:
        // 1. Get existing session
        {
            let response = server
                .s_get_by_id(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::SGetByIdHeaderParams {
                        if_modified_since: None,
                    },
                    &models::SGetByIdPathParams {
                        sid: test_session.api_id.unwrap(),
                    },
                )
                .await
                .unwrap();
            match response {
                SGetByIdResponse::Status200_SuccessfulOperation(session) => {
                    assert_eq!(session.api_id, test_session.api_id);
                    assert_eq!(session.titel, test_session.titel);
                }
                _ => panic!("Expected successful operation response"),
            }
        }

        // 2. Get non-existent session
        {
            let non_existent_id = Uuid::now_v7();
            let response = server
                .s_get_by_id(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::SGetByIdHeaderParams {
                        if_modified_since: None,
                    },
                    &models::SGetByIdPathParams {
                        sid: non_existent_id,
                    },
                )
                .await
                .unwrap();
            assert_eq!(response, SGetByIdResponse::Status404_ContentNotFound);
        }

        // 3. Get session with invalid ID
        {
            let invalid_id = Uuid::nil();
            let response = server
                .s_get_by_id(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::SGetByIdHeaderParams {
                        if_modified_since: None,
                    },
                    &models::SGetByIdPathParams { sid: invalid_id },
                )
                .await
                .unwrap();
            assert_eq!(response, SGetByIdResponse::Status404_ContentNotFound);
        }
        {
            let response = server
                .s_get_by_id(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::SGetByIdHeaderParams {
                        if_modified_since: Some(chrono::Utc::now()),
                    },
                    &models::SGetByIdPathParams {
                        sid: test_session.api_id.unwrap(),
                    },
                )
                .await
                .unwrap();
            assert_eq!(response, SGetByIdResponse::Status304_NotModified);
        }

        // Test cases for s_get:
        // 1. Get sessions with valid parameters
        {
            let response = server
                .s_get(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::SGetHeaderParams {
                        if_modified_since: None,
                    },
                    &models::SGetQueryParams {
                        limit: None,
                        offset: None,
                        p: Some(models::Parlament::Bt),
                        since: None,
                        until: None,
                        wp: Some(20),
                        vgid: None,
                        vgtyp: None,
                    },
                )
                .await
                .unwrap();
            match response {
                SGetResponse::Status200_AntwortAufEineGefilterteAnfrageZuSitzungen(sessions) => {
                    assert!(!sessions.is_empty());
                }
                rsp => panic!("Expected successful operation response, got {:?}", rsp),
            }
        }

        // 2. Get sessions with invalid parameters
        {
            let response = server
                .s_get(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::SGetHeaderParams {
                        if_modified_since: None,
                    },
                    &models::SGetQueryParams {
                        limit: None,
                        offset: None,
                        p: None,
                        since: Some(Utc::now()),
                        until: Some(Utc::now() - chrono::Duration::days(365)),
                        wp: None,
                        vgid: None,
                        vgtyp: None,
                    },
                )
                .await
                .unwrap();
            assert_eq!(response, SGetResponse::Status416_RequestRangeNotSatisfiable);
        }

        let test_session = create_test_session();
        // First create a session with specific parameters
        let create_response = server
            .sid_put(
                &Method::PUT,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(auth::APIScope::Admin, 1),
                &models::SidPutPathParams {
                    sid: test_session.api_id.unwrap(),
                },
                &test_session,
            )
            .await
            .unwrap();
        assert_eq!(create_response, SidPutResponse::Status201_Created);
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        // 3. Get sessions with filters
        {
            let response = server
                .s_get(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::SGetHeaderParams {
                        if_modified_since: None,
                    },
                    &models::SGetQueryParams {
                        limit: Some(10),
                        offset: Some(0),
                        p: Some(models::Parlament::Bt),
                        since: None,
                        until: None,
                        wp: Some(20),
                        vgid: None,
                        vgtyp: None,
                    },
                )
                .await
                .unwrap();
            match response {
                SGetResponse::Status200_AntwortAufEineGefilterteAnfrageZuSitzungen(sessions) => {
                    assert!(!sessions.is_empty());
                }
                _ => panic!("Expected successful operation response"),
            }
        }
        {
            let response = server
                .s_get(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::SGetHeaderParams {
                        if_modified_since: Some(chrono::Utc::now()),
                    },
                    &models::SGetQueryParams {
                        limit: Some(10),
                        offset: Some(0),
                        p: Some(models::Parlament::Bt),
                        since: None,
                        until: None,
                        wp: Some(20),
                        vgid: None,
                        vgtyp: None,
                    },
                )
                .await
                .unwrap();
            assert_eq!(response, SGetResponse::Status304_NoNewChanges);
        }
        {
            let response = server
                .s_get(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::SGetHeaderParams {
                        if_modified_since: None,
                    },
                    &models::SGetQueryParams {
                        limit: Some(10),
                        offset: Some(0),
                        p: Some(models::Parlament::Bt),
                        since: None,
                        until: None,
                        wp: Some(22),
                        vgid: None,
                        vgtyp: None,
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                SGetResponse::Status204_NoContentFoundForTheSpecifiedParameters
            );
        }
        // Cleanup
        cleanup_server("test_session_get").await.unwrap();
    }

    #[tokio::test]
    async fn test_session_modify_endpoints() {
        let server = setup_server("session_modify_ep").await.unwrap();
        let sitzung = create_test_session();
        // - Input non-existing session
        {
            let response = server
                .sid_put(
                    &Method::PUT,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(auth::APIScope::Admin, 1),
                    &models::SidPutPathParams {
                        sid: sitzung.api_id.unwrap(),
                    },
                    &sitzung,
                )
                .await
                .unwrap();
            assert_eq!(response, SidPutResponse::Status201_Created);
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        }

        // - Update existing session with the same data
        let response = server
            .sid_put(
                &Method::PUT,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(auth::APIScope::Admin, 1),
                &models::SidPutPathParams {
                    sid: sitzung.api_id.unwrap(),
                },
                &sitzung,
            )
            .await
            .unwrap();
        assert_eq!(
            response,
            SidPutResponse::Status204_NotModified,
            "Failed to update existing session with the same data.\nInput:  {:?}\n\n Output: {:?}",
            sitzung,
            server
                .s_get_by_id(
                    &Method::PUT,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::SGetByIdHeaderParams {
                        if_modified_since: None
                    },
                    &models::SGetByIdPathParams {
                        sid: sitzung.api_id.unwrap()
                    },
                )
                .await
                .unwrap()
        );
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        // - Update existing session with valid new data
        let rsp_new = models::Sitzung {
            link: Some("https://example.com/a/b/c".to_string()),
            ..sitzung.clone()
        };
        let response = server
            .sid_put(
                &Method::PUT,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(auth::APIScope::Admin, 1),
                &models::SidPutPathParams {
                    sid: sitzung.api_id.unwrap(),
                },
                &rsp_new,
            )
            .await
            .unwrap();
        assert_eq!(response, SidPutResponse::Status201_Created);
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        // - Update session with insufficient permissions
        let response = server
            .sid_put(
                &Method::PUT,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(auth::APIScope::Collector, 1),
                &models::SidPutPathParams {
                    sid: sitzung.api_id.unwrap(),
                },
                &sitzung,
            )
            .await
            .unwrap();
        assert_eq!(response, SidPutResponse::Status401_APIKeyIsMissingOrInvalid);

        // Test cases for sitzung_delete:
        {
            // - Delete existing session with proper permissions
            let response = server
                .sitzung_delete(
                    &Method::PUT,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(auth::APIScope::KeyAdder, 1),
                    &models::SitzungDeletePathParams {
                        sid: sitzung.api_id.unwrap(),
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                SitzungDeleteResponse::Status204_DeletedSuccessfully
            );

            // - Delete non-existent session
            let response = server
                .sitzung_delete(
                    &Method::PUT,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(auth::APIScope::KeyAdder, 1),
                    &models::SitzungDeletePathParams {
                        sid: sitzung.api_id.unwrap(),
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                SitzungDeleteResponse::Status404_NoElementWithThisID
            );

            // - Delete session with insufficient permissions
            let response = server
                .sitzung_delete(
                    &Method::PUT,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(auth::APIScope::Collector, 1),
                    &models::SitzungDeletePathParams {
                        sid: sitzung.api_id.unwrap(),
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                SitzungDeleteResponse::Status401_APIKeyIsMissingOrInvalid
            );
        }
        cleanup_server("session_modify_ep").await.unwrap();
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
