use crate::db::{delete, insert, merge, retrieve};
use crate::error::{DataValidationError, LTZFError};
use crate::utils::as_option;
use crate::{LTZFServer, Result};
use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::{CookieJar, Host};
use openapi::apis::{
    collector_schnittstellen_vorgang::*, data_administration_vorgang::*, unauthorisiert_vorgang::*,
};
use openapi::models;
use uuid::Uuid;

use super::auth::{self, APIScope};
use super::compare::*;
use super::find_applicable_date_range;
use crate::db;

#[async_trait]
impl DataAdministrationVorgang<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;
    #[doc = "VorgangDelete - DELETE /api/v2/vorgang/{vorgang_id}"]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_delete(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::VorgangDeletePathParams,
    ) -> Result<VorgangDeleteResponse> {
        if claims.0 != auth::APIScope::Admin && claims.0 != auth::APIScope::KeyAdder {
            return Ok(VorgangDeleteResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        db::delete::delete_vorgang_by_api_id(path_params.vorgang_id, self).await
    }

    #[doc = "VorgangIdPut - PUT /api/v2/vorgang/{vorgang_id}"]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_id_put(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::VorgangIdPutPathParams,
        body: &models::Vorgang,
    ) -> Result<VorgangIdPutResponse> {
        if claims.0 != auth::APIScope::Admin && claims.0 != auth::APIScope::KeyAdder {
            return Ok(VorgangIdPutResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let mut tx = self.sqlx_db.begin().await?;
        let api_id = path_params.vorgang_id;
        let db_id = sqlx::query!("SELECT id FROM vorgang WHERE api_id = $1", api_id)
            .map(|x| x.id)
            .fetch_optional(&mut *tx)
            .await?;
        match db_id {
            Some(db_id) => {
                let db_cmpvg = retrieve::vorgang_by_id(db_id, &mut tx).await?;
                if compare_vorgang(&db_cmpvg, body) {
                    return Ok(VorgangIdPutResponse::Status304_NotModified {
                        x_rate_limit_limit: None,
                        x_rate_limit_remaining: None,
                        x_rate_limit_reset: None,
                    });
                }
                match delete::delete_vorgang_by_api_id(api_id, self).await? {
                    VorgangDeleteResponse::Status204_NoContent { .. } => {
                        insert::insert_vorgang(body, Uuid::nil(), claims.1, &mut tx, self).await?;
                    }
                    _ => {
                        unreachable!("If this is reached, some assumptions did not hold")
                    }
                }
            }
            None => {
                insert::insert_vorgang(body, Uuid::nil(), claims.1, &mut tx, self).await?;
            }
        }
        tx.commit().await?;
        Ok(VorgangIdPutResponse::Status201_Created {
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
        })
    }
}

#[async_trait]
impl CollectorSchnittstellenVorgang<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;

    #[doc = "VorgangPut - PUT /api/v2/vorgang"]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_put(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        header_params: &models::VorgangPutHeaderParams,
        body: &models::Vorgang,
    ) -> Result<VorgangPutResponse> {
        // technically not necessary since all authenticated scopes are allowed, still, better be explicit about that
        if claims.0 != APIScope::KeyAdder
            && claims.0 != APIScope::Admin
            && claims.0 != APIScope::Collector
        {
            return Ok(VorgangPutResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let rval =
            merge::execute::run_integration(body, header_params.x_scraper_id, claims.1, self).await;
        match rval {
            Ok(_) => Ok(VorgangPutResponse::Status201_Created {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            }),
            Err(e) => match &e {
                LTZFError::Validation { source } => match **source {
                    DataValidationError::AmbiguousMatch { .. } => {
                        Ok(VorgangPutResponse::Status409_Conflict {
                            x_rate_limit_limit: None,
                            x_rate_limit_remaining: None,
                            x_rate_limit_reset: None,
                        })
                    }
                    _ => Err(e),
                },
                _ => Err(e),
            },
        }
    }
}

#[async_trait]
impl UnauthorisiertVorgang<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;
    #[doc = "VorgangGetById - GET /api/v2/vorgang/{vorgang_id}"]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_get_by_id(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        header_params: &models::VorgangGetByIdHeaderParams,
        path_params: &models::VorgangGetByIdPathParams,
    ) -> Result<VorgangGetByIdResponse> {
        tracing::trace!(
            "vorgang_get_by_id called with id {}",
            path_params.vorgang_id
        );
        let mut tx = self.sqlx_db.begin().await?;
        let exists = sqlx::query!(
            "SELECT 1 as out FROM vorgang WHERE api_id = $1",
            path_params.vorgang_id
        )
        .fetch_optional(&mut *tx)
        .await?
        .is_some();
        if !exists {
            return Ok(VorgangGetByIdResponse::Status404_NotFound {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let dbid = sqlx::query!(
            "SELECT id FROM vorgang WHERE api_id = $1 AND EXISTS (
                SELECT 1 FROM station s WHERE s.zp_modifiziert > COALESCE($2::timestamptz, '1940-01-01T00:00:00Z') AND s.vg_id = vorgang.id
            )",
            path_params.vorgang_id,
            header_params.if_modified_since
        )
        .map(|x| x.id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(dbid) = dbid {
            let mut result = retrieve::vorgang_by_id(dbid, &mut tx).await?;
            if claims.0 == APIScope::Admin || claims.0 == APIScope::KeyAdder {
                result.touched_by = as_option(
                    sqlx::query!(
                        "SELECT * FROM scraper_touched_vorgang sts
                INNER JOIN api_keys ON api_keys.id = sts.collector_key
                WHERE vg_id = $1",
                        dbid
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
            Ok(VorgangGetByIdResponse::Status200_Success {
                body: result,
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            })
        } else {
            return Ok(VorgangGetByIdResponse::Status304_NotModified {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
    }

    #[doc = "VorgangGet - GET /api/v2/vorgang"]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_get(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        header_params: &models::VorgangGetHeaderParams,
        query_params: &models::VorgangGetQueryParams,
    ) -> Result<VorgangGetResponse> {
        let mut tx = self.sqlx_db.begin().await?;
        if let Some(range) = find_applicable_date_range(
            None,
            None,
            None,
            query_params.since,
            query_params.until,
            header_params.if_modified_since,
        ) {
            let parameters = retrieve::VGGetParameters {
                lower_date: range.since,
                parlament: query_params.p,
                upper_date: range.until,
                vgtyp: query_params.vgtyp,
                wp: query_params.wp,
                inifch: query_params.fach.clone(),
                iniorg: query_params.org.clone(),
                inipsn: query_params.person.clone(),
            };
            let result = retrieve::vorgang_by_parameter(
                parameters,
                query_params.page,
                query_params.per_page,
                &mut tx,
            )
            .await?;
            if result.1.is_empty() && header_params.if_modified_since.is_none() {
                tx.rollback().await?;
                Ok(VorgangGetResponse::Status204_NoContent {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None,
                })
            } else if result.1.is_empty() && header_params.if_modified_since.is_some() {
                tx.rollback().await?;
                Ok(VorgangGetResponse::Status304_NotModified {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None,
                })
            } else {
                tx.commit().await?;
                let prp = &result.0;
                Ok(VorgangGetResponse::Status200_Successful {
                    body: result.1,
                    x_total_count: Some(prp.x_total_count),
                    x_total_pages: Some(prp.x_total_pages),
                    x_page: Some(prp.x_page),
                    x_per_page: Some(prp.x_per_page),
                    link: Some(prp.generate_link_header("/api/v2/vorgang")),
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None,
                })
            }
        } else {
            tx.rollback().await?;
            Ok(VorgangGetResponse::Status416_RequestRangeNotSatisfiable {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            })
        }
    }
}

#[cfg(test)]
mod test_endpoints {

    use axum::http::Method;
    use axum_extra::extract::{CookieJar, Host};
    use chrono::Utc;
    use openapi::apis::collector_schnittstellen_vorgang::*;
    use openapi::apis::data_administration_vorgang::*;
    use openapi::apis::unauthorisiert_vorgang::*;

    use openapi::models;
    use openapi::models::VorgangIdPutPathParams;
    use openapi::models::VorgangPutHeaderParams;
    use uuid::Uuid;

    use crate::api::auth;
    use crate::api::auth::APIScope;
    use crate::utils::test::TestSetup;

    use super::super::endpoint_test::*;
    // Procedure (Vorgang) tests
    #[tokio::test]
    async fn test_vorgang_get_by_id_endpoints() {
        // Setup test server and database
        let scenario = TestSetup::new("test_vorgang_by_id_get").await;
        let server = &scenario.server;
        let test_vorgang = create_test_vorgang();
        // First create the procedure
        let create_response = server
            .vorgang_put(
                &Method::PUT,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(auth::APIScope::Collector, 1),
                &models::VorgangPutHeaderParams {
                    x_scraper_id: test_vorgang.api_id,
                },
                &test_vorgang,
            )
            .await
            .unwrap();
        assert_eq!(
            create_response,
            VorgangPutResponse::Status201_Created {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            }
        );
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        // Test cases for vorgang_get_by_id:
        // 1. Get existing procedure
        {
            let response = server
                .vorgang_get_by_id(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(APIScope::Collector, 1),
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
                VorgangGetByIdResponse::Status200_Success { body, .. } => {
                    assert_eq!(body.api_id, test_vorgang.api_id);
                    assert_eq!(body.titel, test_vorgang.titel);
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
                    &(APIScope::Collector, 1),
                    &models::VorgangGetByIdHeaderParams {
                        if_modified_since: None,
                    },
                    &models::VorgangGetByIdPathParams {
                        vorgang_id: non_existent_id,
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                VorgangGetByIdResponse::Status404_NotFound {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None,
                }
            );
        }

        // 3. Get procedure with invalid ID
        {
            let invalid_id = Uuid::nil();
            let response = server
                .vorgang_get_by_id(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(APIScope::Collector, 1),
                    &models::VorgangGetByIdHeaderParams {
                        if_modified_since: None,
                    },
                    &models::VorgangGetByIdPathParams {
                        vorgang_id: invalid_id,
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                VorgangGetByIdResponse::Status404_NotFound {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
            );
        }
        let response = server
            .vorgang_get_by_id(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::Collector, 1),
                &models::VorgangGetByIdHeaderParams {
                    if_modified_since: Some(chrono::Utc::now()),
                },
                &models::VorgangGetByIdPathParams {
                    vorgang_id: test_vorgang.api_id,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            response,
            VorgangGetByIdResponse::Status304_NotModified {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            }
        );
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn test_vorgang_get_filtered_endpoints_empty() {
        let scenario = TestSetup::new("test_vorgang_get_filtered_empty").await;
        let server = &scenario.server;
        let test_vorgang = create_test_vorgang();
        // First create the procedure
        {
            let create_response = server
                .vorgang_put(
                    &Method::PUT,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(auth::APIScope::Collector, 1),
                    &models::VorgangPutHeaderParams {
                        x_scraper_id: test_vorgang.api_id,
                    },
                    &test_vorgang,
                )
                .await
                .unwrap();
            assert_eq!(
                create_response,
                VorgangPutResponse::Status201_Created {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                },
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
                        page: None,
                        per_page: None,
                        p: None,
                        since: Some(Utc::now()),
                        until: Some(Utc::now() - chrono::Duration::days(365)), // invalid: until is before since
                        vgtyp: None,
                        wp: None,
                        fach: None,
                        org: None,
                        person: None,
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                VorgangGetResponse::Status416_RequestRangeNotSatisfiable {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
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
                        page: None,
                        per_page: None,
                        p: None,
                        since: Some(Utc::now() + chrono::Duration::days(365)),
                        until: Some(Utc::now() + chrono::Duration::days(366)),
                        vgtyp: None,
                        wp: None,
                        fach: None,
                        org: None,
                        person: None,
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                VorgangGetResponse::Status204_NoContent {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
            );
        }
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn test_vorgang_get_filtered_endpoints_success() {
        let scenario = TestSetup::new("test_vorgang_get_filtered_success").await;
        let server = &scenario.server;
        let test_vorgang = create_test_vorgang();
        {
            // First create a procedure with specific parameters
            let create_response = server
                .vorgang_put(
                    &Method::PUT,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(auth::APIScope::Collector, 1),
                    &models::VorgangPutHeaderParams {
                        x_scraper_id: test_vorgang.api_id,
                    },
                    &test_vorgang,
                )
                .await
                .unwrap();
            assert_eq!(
                create_response,
                VorgangPutResponse::Status201_Created {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None,
                }
            );
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        }
        {
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
                        page: Some(0),
                        per_page: Some(32),
                        p: Some(models::Parlament::Bb),
                        since: None,
                        until: None,
                        vgtyp: Some(test_vorgang.typ),
                        wp: Some(test_vorgang.wahlperiode as i32),
                        fach: None,
                        org: None,
                        person: None,
                    },
                )
                .await
                .unwrap();
            match response {
                VorgangGetResponse::Status200_Successful { body, .. } => {
                    assert!(!body.is_empty());
                }
                response => panic!("Expected successful operation response, got {response:?}"),
            }
        }

        // Cleanup
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn test_vorgang_put_endpoint() {
        // Setup test server and database
        let scenario = TestSetup::new("test_vorgang_put").await;
        let server = &scenario.server;
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
            assert_eq!(
                response,
                VorgangIdPutResponse::Status201_Created {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
            );
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
                VorgangIdPutResponse::Status403_Forbidden {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
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
                    &models::VorgangPutHeaderParams {
                        x_scraper_id: test_vorgang.api_id,
                    },
                    &test_vorgang,
                )
                .await
                .unwrap();
            assert_eq!(
                response,
                VorgangPutResponse::Status201_Created {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
            );
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
            assert_eq!(
                rsp1,
                VorgangIdPutResponse::Status201_Created {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
            );

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
            assert_eq!(
                rsp2,
                VorgangIdPutResponse::Status201_Created {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
            );

            let conflict_resp = server
                .vorgang_put(
                    &Method::PUT,
                    &host,
                    &cookies,
                    &(APIScope::Admin, 1),
                    &VorgangPutHeaderParams {
                        x_scraper_id: Uuid::nil(),
                    },
                    &vg3,
                )
                .await
                .unwrap();
            assert_eq!(
                conflict_resp,
                VorgangPutResponse::Status409_Conflict {
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
    async fn test_vorgang_delete_endpoints() {
        // Setup test server and database
        let scenario = TestSetup::new("test_vorgang_delete").await;
        let server = &scenario.server;

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
                    &models::VorgangPutHeaderParams {
                        x_scraper_id: Uuid::now_v7(),
                    },
                    &test_vorgang,
                )
                .await
                .unwrap();
            assert_eq!(
                create_response,
                VorgangPutResponse::Status201_Created {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
            );

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
                VorgangDeleteResponse::Status204_NoContent {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                },
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
                VorgangDeleteResponse::Status404_NotFound {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None
                }
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
                VorgangDeleteResponse::Status403_Forbidden {
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
    async fn test_malformed_data_vorgang() {
        // TODO test multiple conflicting stations
    }

    #[tokio::test]
    async fn test_malformed_data_station() {
        // TODO test multiple conflicting dokumente / stellungnahmen
    }
}

#[cfg(test)]
mod test_failed_irl_scenarios {
    use crate::{api::auth::APIScope, utils::test::TestSetup};
    use axum::http::Method;
    use axum_extra::extract::{CookieJar, Host};
    use openapi::{
        apis::collector_schnittstellen_vorgang::{
            CollectorSchnittstellenVorgang, VorgangPutResponse,
        },
        models,
    };
    use std::path::{Path, PathBuf};
    use tokio::io::{AsyncBufReadExt, BufReader};
    use uuid::Uuid;

    async fn read_jsonl(path: &Path) -> Vec<models::Vorgang> {
        let file = tokio::fs::File::open(path).await.unwrap();
        let buf_reader = BufReader::new(file);
        let mut records = vec![];
        let mut lines = buf_reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            records.push(serde_json::from_str(&line).unwrap())
        }
        return records;
    }

    macro_rules! irl_scenario {
        ($path:expr, $name:ident) => {
            #[tokio::test]
            async fn $name() {
                let path = PathBuf::from($path);
                let test_setup =
                    TestSetup::new(&concat!("scenario_test_", stringify!($name))).await;
                let host = Host("localhost".to_string());
                let cookies = CookieJar::new();
                let objects = read_jsonl(&path).await;
                for obj in objects.iter() {
                    let response = test_setup
                        .server
                        .vorgang_put(
                            &Method::PUT,
                            &host,
                            &cookies,
                            &(APIScope::KeyAdder, 1),
                            &models::VorgangPutHeaderParams {
                                x_scraper_id: Uuid::nil(),
                            },
                            obj,
                        )
                        .await
                        .unwrap();
                    assert!(matches!(
                        response,
                        VorgangPutResponse::Status201_Created { .. }
                    ));
                }
                test_setup.teardown().await;
            }
        };
    }
    irl_scenario!(
        "tests/scenarios/on_onflict_upd_nodouble.jsonl",
        test_scenario_on_onflict_upd_nodouble
    );
}
