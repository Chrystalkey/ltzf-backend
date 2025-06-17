use crate::db::retrieve::{SitzungFilterParameters, sitzung_by_param};
use crate::db::{delete, insert, retrieve};
use crate::error::LTZFError;
use crate::utils::as_option;
use crate::{LTZFServer, Result};
use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::{CookieJar, Host};
use chrono::Datelike;
use openapi::apis::collector_schnittstellen_sitzung::*;
use openapi::apis::data_administration_sitzung::*;
use openapi::apis::sitzung_unauthorisiert::*;
use openapi::models;
use uuid::Uuid;

use super::auth::{self, APIScope};
use super::{compare::*, find_applicable_date_range};

#[async_trait]
impl DataAdministrationSitzung<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;
    #[doc = "SitzungDelete - DELETE /api/v1/sitzung/{sid}"]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn sitzung_delete(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::SitzungDeletePathParams,
    ) -> Result<SitzungDeleteResponse> {
        if claims.0 != auth::APIScope::Admin && claims.0 != auth::APIScope::KeyAdder {
            return Ok(SitzungDeleteResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        Ok(delete::delete_sitzung_by_api_id(path_params.sid, self).await?)
    }

    #[doc = "SidPut - PUT /api/v1/sitzung/{sid}"]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn sid_put(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::SidPutPathParams,
        body: &models::Sitzung,
    ) -> Result<SidPutResponse> {
        if claims.0 != auth::APIScope::Admin && claims.0 != auth::APIScope::KeyAdder {
            return Ok(SidPutResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let mut tx = self.sqlx_db.begin().await?;
        let api_id = path_params.sid;
        let db_id = sqlx::query!("SELECT id FROM sitzung WHERE api_id = $1", api_id)
            .map(|x| x.id)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(db_id) = db_id {
            let db_cmpvg = retrieve::sitzung_by_id(db_id, &mut tx).await?;
            if compare_sitzung(&db_cmpvg, body) {
                return Ok(SidPutResponse::Status304_NotModified {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None,
                });
            }
            match delete::delete_sitzung_by_api_id(api_id, self).await? {
                SitzungDeleteResponse::Status204_NoContent { .. } => {
                    insert::insert_sitzung(body, Uuid::nil(), claims.1, &mut tx, self).await?;
                }
                _ => {
                    unreachable!("If this is reached, some assumptions did not hold")
                }
            }
        } else {
            insert::insert_sitzung(body, Uuid::nil(), claims.1, &mut tx, self).await?;
        }
        tx.commit().await?;
        Ok(SidPutResponse::Status201_Created {
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
        })
    }
}

#[async_trait]
impl CollectorSchnittstellenSitzung<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;

    #[doc = "KalDatePut - PUT /api/v1/kalender/{parlament}/{datum}"]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn kal_date_put(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        header_params: &models::KalDatePutHeaderParams,
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
            return Ok(KalDatePutResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
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

        let dt_begin = path_params
            .datum
            .and_time(chrono::NaiveTime::from_hms_micro_opt(0, 0, 0, 0).unwrap())
            .and_utc();
        let dt_end = path_params
            .datum
            .checked_add_days(chrono::Days::new(1))
            .unwrap()
            .and_time(chrono::NaiveTime::from_hms_micro_opt(0, 0, 0, 0).unwrap())
            .and_utc();
        // delete all entries that fit the description
        sqlx::query!(
            "DELETE FROM sitzung WHERE sitzung.id = ANY(SELECT s.id FROM sitzung s 
        INNER JOIN gremium g ON g.id=s.gr_id 
        INNER JOIN parlament p ON p.id=g.parl 
        WHERE p.value = $1 AND s.termin BETWEEN $2 AND $3)",
            path_params.parlament.to_string(),
            dt_begin,
            dt_end
        )
        .execute(&mut *tx)
        .await?;

        // insert all entries
        for s in &body {
            insert::insert_sitzung(s, header_params.x_scraper_id, claims.1, &mut tx, self).await?;
        }
        tx.commit().await?;
        Ok(KalDatePutResponse::Status201_Created {
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
        })
    }
}

#[async_trait]
impl SitzungUnauthorisiert<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;
    #[doc = "KalDateGet - GET /api/v1/kalender/{parlament}/{datum}"]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn kal_date_get(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        header_params: &models::KalDateGetHeaderParams,
        path_params: &models::KalDateGetPathParams,
        query_params: &models::KalDateGetQueryParams,
    ) -> Result<KalDateGetResponse> {
        let mut tx = self.sqlx_db.begin().await?;
        let dr = find_applicable_date_range(
            Some(path_params.datum.year() as u32),
            Some(path_params.datum.month()),
            Some(path_params.datum.day()),
            None,
            None,
            header_params.if_modified_since,
        );
        if dr.is_none() {
            return Ok(KalDateGetResponse::Status404_NotFound {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let dr = dr.unwrap();

        let dt_begin = dr.since;
        let dt_end = dr.until;
        let result = sitzung_by_param(
            &SitzungFilterParameters {
                parlament: Some(path_params.parlament),
                gremium_like: None,
                since: dt_begin,
                until: dt_end,
                vgid: None,
                wp: None,
            },
            query_params.page,
            query_params.per_page,
            &mut tx,
        )
        .await?;

        let prp = &result.0;

        if result.1.is_empty() {
            tx.rollback().await?;
            return Ok(KalDateGetResponse::Status404_NotFound {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        tx.commit().await?;
        Ok(KalDateGetResponse::Status200_SuccessfulResponse {
            body: result.1,
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
            link: Some(prp.generate_link_header(&format!(
                "/api/v1/kalender/{}/{}",
                path_params.parlament, path_params.datum
            ))),
            x_page: Some(prp.x_page),
            x_per_page: Some(prp.x_per_page),
            x_total_count: Some(prp.x_total_count),
            x_total_pages: Some(prp.x_total_pages),
        })
    }

    /// TODO: unify kal_get and kal_date_get by utilising sitzung_retrieve_by_param
    /// find a way to implement pagination and the prp here
    #[doc = "KalGet - GET /api/v1/kalender"]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn kal_get(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        header_params: &models::KalGetHeaderParams,
        query_params: &models::KalGetQueryParams,
    ) -> Result<KalGetResponse> {
        let qparams = query_params;
        let hparams = header_params;
        let mut tx = self.sqlx_db.begin().await?;
        let result = find_applicable_date_range(
            qparams.y.map(|x| x as u32),
            qparams.m.map(|x| x as u32),
            qparams.dom.map(|x| x as u32),
            qparams.since,
            qparams.until,
            hparams.if_modified_since,
        );
        if result.is_none() {
            return Ok(KalGetResponse::Status416_RequestRangeNotSatisfiable {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }

        let params = retrieve::SitzungFilterParameters {
            gremium_like: qparams.gr.clone(),
            parlament: qparams.p,
            vgid: None,
            wp: qparams.wp.map(|x| x as u32),
            since: result.as_ref().unwrap().since,
            until: result.unwrap().until,
        };

        // retrieval
        let result =
            retrieve::sitzung_by_param(&params, query_params.page, query_params.per_page, &mut tx)
                .await?;
        if result.1.is_empty() {
            tx.rollback().await?;
            Ok(KalGetResponse::Status204_NoContent {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            })
        } else if result.1.is_empty() && header_params.if_modified_since.is_some() {
            Ok(KalGetResponse::Status304_NotModified {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            })
        } else {
            tx.commit().await?;
            let prp = &result.0;
            Ok(KalGetResponse::Status200_SuccessfulResponse {
                body: result.1,
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
                x_total_count: Some(prp.x_total_count),
                x_total_pages: Some(prp.x_total_pages),
                x_page: Some(prp.x_page),
                x_per_page: Some(prp.x_per_page),
                link: Some(prp.generate_link_header("/api/v1/kalender")),
            })
        }
    }

    #[doc = "SGetById - GET /api/v1/sitzung/{sid}"]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn s_get_by_id(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        header_params: &models::SGetByIdHeaderParams,
        path_params: &models::SGetByIdPathParams,
    ) -> Result<SGetByIdResponse> {
        let mut tx = self.sqlx_db.begin().await?;
        let api_id = path_params.sid;
        let id_exists = sqlx::query!("SELECT 1 as x FROM sitzung WHERE api_id = $1", api_id)
            .fetch_optional(&mut *tx)
            .await?;
        if id_exists.is_none() {
            return Ok(SGetByIdResponse::Status404_NotFound {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }

        let id = sqlx::query!(
            "
        SELECT id FROM sitzung WHERE api_id = $1
        AND last_update > COALESCE($2, CAST('1940-01-01T00:00:00' AS TIMESTAMPTZ));",
            api_id,
            header_params.if_modified_since
        )
        .map(|r| r.id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(id) = id {
            let mut result = retrieve::sitzung_by_id(id, &mut tx).await?;
            if claims.0 == APIScope::KeyAdder || claims.0 == APIScope::Admin {
                result.touched_by = as_option(
                    sqlx::query!(
                        "SELECT * FROM scraper_touched_sitzung sts
                    INNER JOIN api_keys ON api_keys.id = sts.collector_key
                    WHERE sid = $1",
                        id
                    )
                    .map(|r| models::TouchedByInner {
                        key: Some(r.key_hash),
                        scraper_id: Some(r.scraper),
                    })
                    .fetch_all(&mut *tx)
                    .await?,
                );
            }
            tx.commit().await?;
            Ok(SGetByIdResponse::Status200_Success {
                body: result,
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            })
        } else if header_params.if_modified_since.is_some() {
            Ok(SGetByIdResponse::Status304_NotModified {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            })
        } else {
            Ok(SGetByIdResponse::Status404_NotFound {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            })
        }
    }

    #[doc = "SGet - GET /api/v1/sitzung"]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn s_get(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        header_params: &models::SGetHeaderParams,
        query_params: &models::SGetQueryParams,
    ) -> Result<SGetResponse> {
        let range = find_applicable_date_range(
            None,
            None,
            None,
            query_params.since,
            query_params.until,
            header_params.if_modified_since,
        );
        if range.is_none() {
            return Ok(SGetResponse::Status416_RequestRangeNotSatisfiable {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let params = retrieve::SitzungFilterParameters {
            gremium_like: None,
            parlament: query_params.p,
            wp: query_params.wp.map(|x| x as u32),
            since: range.as_ref().unwrap().since,
            until: range.unwrap().until,
            vgid: query_params.vgid,
        };

        let mut tx: sqlx::Transaction<'_, sqlx::Postgres> = self.sqlx_db.begin().await?;
        let result =
            retrieve::sitzung_by_param(&params, query_params.page, query_params.per_page, &mut tx)
                .await?;
        let prp = result.0;
        tx.commit().await?;
        if result.1.is_empty() && header_params.if_modified_since.is_none() {
            Ok(SGetResponse::Status204_NoContent {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            })
        } else if result.1.is_empty() && header_params.if_modified_since.is_some() {
            Ok(SGetResponse::Status304_NotModified {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            })
        } else {
            Ok(SGetResponse::Status200_SuccessfulResponse {
                body: result.1,
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
                x_total_count: Some(prp.x_total_count),
                x_total_pages: Some(prp.x_total_pages),
                x_page: Some(prp.x_page),
                x_per_page: Some(prp.x_per_page),
                link: Some(prp.generate_link_header("/api/v1/sitzung")),
            })
        }
    }
}

#[cfg(test)]
mod sitzung_test {
    use axum::http::Method;
    use axum_extra::extract::{CookieJar, Host};
    use chrono::Utc;
    use openapi::apis::collector_schnittstellen_sitzung::*;
    use openapi::apis::data_administration_sitzung::*;
    use openapi::apis::sitzung_unauthorisiert::*;

    use openapi::models;
    use uuid::Uuid;

    use crate::api::auth::APIScope;
    use crate::utils::test::TestSetup;

    use super::super::auth;
    use super::super::endpoint_test::*;

    // Calendar tests
    #[tokio::test]
    async fn test_calendar_endpoints() {
        // Setup test server and database
        let scenario = TestSetup::new("test_calendar").await;
        let server = &scenario.server;
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
                    &models::KalDatePutHeaderParams {
                        x_scraper_id: Uuid::nil(),
                    },
                    &models::KalDatePutPathParams {
                        datum: test_date,
                        parlament: models::Parlament::Bt,
                    },
                    &test_sessions,
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                KalDatePutResponse::Status201_Created {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
            );
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
                    &models::KalDatePutHeaderParams {
                        x_scraper_id: Uuid::nil(),
                    },
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
                KalDatePutResponse::Status403_Forbidden {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
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
                    &models::KalDatePutHeaderParams {
                        x_scraper_id: Uuid::nil(),
                    },
                    &models::KalDatePutPathParams {
                        datum: recent_date,
                        parlament: models::Parlament::Bt,
                    },
                    &test_sessions,
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                KalDatePutResponse::Status201_Created {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
            );
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
                    &models::KalDateGetQueryParams {
                        page: None,
                        per_page: None,
                    },
                )
                .await
                .unwrap();
            match response {
                KalDateGetResponse::Status200_SuccessfulResponse { body, .. } => {
                    assert!(!body.is_empty(), "Expected to find at least one session");
                    assert_eq!(body[0].gremium.parlament, models::Parlament::Bt);
                    assert_eq!(
                        body[0].termin.date_naive(),
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
                    &models::KalDateGetQueryParams {
                        page: None,
                        per_page: None,
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                KalDateGetResponse::Status404_NotFound {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
            );
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
                        page: None,
                        per_page: None,
                        y: Some(recent_date.format("%Y").to_string().parse::<i32>().unwrap()),
                        m: Some(recent_date.format("%m").to_string().parse::<i32>().unwrap()),
                        dom: None,
                        gr: None,
                        p: Some(models::Parlament::Bt),
                        since: None,
                        until: None,
                        wp: Some(20),
                    },
                )
                .await
                .unwrap();
            match response {
                KalGetResponse::Status200_SuccessfulResponse { body, .. } => {
                    assert!(
                        !body.is_empty(),
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
                        page: None,
                        per_page: None,
                        y: None,
                        m: None,
                        dom: None,
                        gr: None,
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
                KalGetResponse::Status416_RequestRangeNotSatisfiable {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
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
                        page: None,
                        per_page: None,
                        y: None,
                        m: None,
                        dom: None,
                        gr: None,
                        p: Some(models::Parlament::Bt),
                        since: Some(start_date),
                        until: Some(end_date),
                        wp: None,
                    },
                )
                .await
                .unwrap();
            match response {
                KalGetResponse::Status200_SuccessfulResponse { body, .. } => {
                    assert!(!body.is_empty(), "Expected to find sessions in date range");
                    for session in body {
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
        scenario.teardown().await;
    }

    #[tokio::test]
    pub(crate) async fn test_session_get_endpoints() {
        // Setup test server and database
        let scenario = TestSetup::new("test_session_get").await;
        let server = &scenario.server;

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
        assert_eq!(
            create_response,
            SidPutResponse::Status201_Created {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            }
        );
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        // Test cases for s_get_by_id:
        // 1. Get existing session
        {
            let response = server
                .s_get_by_id(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(APIScope::Collector, 1),
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
                SGetByIdResponse::Status200_Success { body, .. } => {
                    assert_eq!(body.api_id, test_session.api_id);
                    assert_eq!(body.titel, test_session.titel);
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
                    &(APIScope::Collector, 1),
                    &models::SGetByIdHeaderParams {
                        if_modified_since: None,
                    },
                    &models::SGetByIdPathParams {
                        sid: non_existent_id,
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                SGetByIdResponse::Status404_NotFound {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
            );
        }

        // 3. Get session with invalid ID
        {
            let invalid_id = Uuid::nil();
            let response = server
                .s_get_by_id(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(APIScope::Collector, 1),
                    &models::SGetByIdHeaderParams {
                        if_modified_since: None,
                    },
                    &models::SGetByIdPathParams { sid: invalid_id },
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                SGetByIdResponse::Status404_NotFound {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
            );
        }
        {
            let response = server
                .s_get_by_id(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(APIScope::Collector, 1),
                    &models::SGetByIdHeaderParams {
                        if_modified_since: Some(chrono::Utc::now()),
                    },
                    &models::SGetByIdPathParams {
                        sid: test_session.api_id.unwrap(),
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                SGetByIdResponse::Status304_NotModified {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
            );
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
                        page: None,
                        per_page: None,
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
                SGetResponse::Status200_SuccessfulResponse { body, .. } => {
                    assert!(!body.is_empty());
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
                        page: None,
                        per_page: None,
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
            assert_eq!(
                response,
                SGetResponse::Status416_RequestRangeNotSatisfiable {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
            );
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
        assert_eq!(
            create_response,
            SidPutResponse::Status201_Created {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            }
        );
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
                        page: None,
                        per_page: None,
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
                SGetResponse::Status200_SuccessfulResponse { body, .. } => {
                    assert!(!body.is_empty());
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
                        page: None,
                        per_page: None,
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
            assert_eq!(
                response,
                SGetResponse::Status304_NotModified {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
            );
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
                        page: None,
                        per_page: None,
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
                SGetResponse::Status204_NoContent {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
            );
        }
        // Cleanup
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn test_session_modify_endpoints() {
        let scenario = TestSetup::new("session_modify_ep").await;
        let server = &scenario.server;
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
            assert_eq!(
                response,
                SidPutResponse::Status201_Created {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
            );
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
            SidPutResponse::Status304_NotModified {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            },
            "Failed to update existing session with the same data.\nInput:  {:?}\n\n Output: {:?}",
            sitzung,
            server
                .s_get_by_id(
                    &Method::PUT,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(APIScope::Collector, 1),
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
        assert_eq!(
            response,
            SidPutResponse::Status201_Created {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            }
        );
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
        assert_eq!(
            response,
            SidPutResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            }
        );

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
                SitzungDeleteResponse::Status204_NoContent {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
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
                SitzungDeleteResponse::Status404_NotFound {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
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
                SitzungDeleteResponse::Status403_Forbidden {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
            );
        }
        scenario.teardown().await;
    }
}
