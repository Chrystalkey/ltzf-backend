use super::RoundTimestamp;
use crate::db::retrieve::{SitzungFilterParameters, sitzung_by_param};
use crate::db::{delete, insert, retrieve};
use crate::error::LTZFError;
use crate::utils::as_option;
use crate::{LTZFServer, Result};
use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::CookieJar;
use chrono::Datelike;
use headers::Host;
use openapi::apis::collector_schnittstellen_sitzung::*;
use openapi::apis::data_administration_sitzung::*;
use openapi::apis::sitzung_unauthorisiert::*;
use openapi::models;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use super::auth::{self, APIScope};
use super::find_applicable_date_range;

// helper that converts the documents in a sitzung into just their uuids instead of full objects
fn st_to_uuiddoks(st: &models::Sitzung) -> models::Sitzung {
    let mut st = st.clone();
    for t in &mut st.tops {
        if t.dokumente.is_none() {
            continue;
        }
        for d in t.dokumente.as_mut().unwrap() {
            if let models::StationDokumenteInner::Dokument(dok) = d {
                *d = models::StationDokumenteInner::String(dok.api_id.unwrap().to_string());
            }
        }
    }
    if st.dokumente.is_none() {
        return st;
    }
    for d in st.dokumente.as_mut().unwrap() {
        if let models::StationDokumenteInner::Dokument(dok) = d {
            *d = models::StationDokumenteInner::String(dok.api_id.unwrap().to_string());
        }
    }
    st
}

#[async_trait]
impl DataAdministrationSitzung<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;
    #[doc = "SitzungDelete - DELETE /api/v2/sitzung/{sid}"]
    #[instrument(skip_all, fields(claim=%claims.0, sid=%path_params.sid))]
    async fn sitzung_delete(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::SitzungDeletePathParams,
    ) -> Result<SitzungDeleteResponse> {
        if claims.0 != auth::APIScope::Admin && claims.0 != auth::APIScope::KeyAdder {
            warn!("Permission Level too low");
            return Ok(SitzungDeleteResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let r = delete::delete_sitzung_by_api_id(path_params.sid, self).await?;
        info!(target: "obj", "Deleted Sitzung {}", path_params.sid);
        info!("Success");
        Ok(r)
    }

    /// PUTs a models::Sitzung into the database with checks on whether
    /// the objects modifies internal state.
    /// NOTE: Documents that are referenced by UUID (within body.dokumente)
    /// and point to a document that is not in the database are silently
    /// filtered out.
    #[doc = "SidPut - PUT /api/v2/sitzung/{sid}"]
    #[instrument(skip_all, fields(claim=%claims.0, sid=%path_params.sid))]
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
            warn!("Permission Level too low");
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
            debug!(
                "odb: {}\nonew: {}",
                serde_json::to_string(&db_cmpvg.with_round_timestamps()).unwrap(),
                serde_json::to_string(&st_to_uuiddoks(body).with_round_timestamps()).unwrap()
            );
            if db_cmpvg.with_round_timestamps() == st_to_uuiddoks(body).with_round_timestamps() {
                info!("Sitzung has the same state as the input object");
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
                    error!("Delete was unsuccessful despite session being in the database");
                    unreachable!("If this is reached, some assumptions did not hold")
                }
            }
        } else {
            insert::insert_sitzung(body, Uuid::nil(), claims.1, &mut tx, self).await?;
        }
        tx.commit().await?;
        info!(target: "obj", "PUT Sitzung {}", api_id);
        info!("Successfully PUT session into database");
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

    #[doc = "KalDatePut - PUT /api/v2/kalender/{parlament}/{datum}"]
    #[instrument(skip_all, fields(claim=%claims.0, date=%path_params.datum))]
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
        if claims.0 == APIScope::Collector && path_params.datum < last_upd_day {
            warn!(
                "Permission not Granted because you are only a collector and {} < {}",
                path_params.datum, last_upd_day
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
            .filter(|&f| {
                f.termin.date_naive() >= last_upd_day
                    && f.gremium.parlament == path_params.parlament
            })
            .cloned()
            .collect();

        if len != body.len() {
            debug!(
                "Filtered {}/{} Sitzungen due to date and parlament equality constraints",
                len - body.len(),
                len
            );
        }
        if len == 0 {
            warn!("Body was empty");
        }
        if body.is_empty() {
            return Ok(KalDatePutResponse::Status201_Created {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
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
        debug!("Deleting entries from {dt_begin} until {dt_end}");
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
        info!(target: "obj", "Inserted sitzungen into db: {:?}", body);
        info!("Inserted {} sessions into the database", body.len());
        Ok(KalDatePutResponse::Status201_Created {
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
        })
    }
}

#[async_trait]
impl SitzungUnauthorisiert<LTZFError> for LTZFServer {
    #[doc = "KalDateGet - GET /api/v2/kalender/{parlament}/{datum}"]
    #[instrument(skip_all, fields(date=%path_params.datum))]
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
            info!("Date Range too narrow or invalid");
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

        if result.1.is_empty() {
            tx.rollback().await?;
            info!("No Sitzungen found in date range {}", dr);
            return Ok(KalDateGetResponse::Status404_NotFound {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        tx.commit().await?;

        let prp = &result.0;
        info!("Successfully fetched Sitzungen");
        Ok(KalDateGetResponse::Status200_SuccessfulResponse {
            body: result.1,
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
            link: Some(prp.generate_link_header(&format!(
                "/api/v2/kalender/{}/{}",
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
    #[doc = "KalGet - GET /api/v2/kalender"]
    #[instrument(skip_all)]
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
            warn!(
                "Parameters were chosen such that the request is unsatisfiable: {:?}, ims={:?}",
                query_params, header_params.if_modified_since
            );
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
        if result.1.is_empty() && header_params.if_modified_since.is_none() {
            info!("No Sitzungen found");
            Ok(KalGetResponse::Status204_NoContent {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            })
        } else if result.1.is_empty() && header_params.if_modified_since.is_some() {
            info!("All results remain unchanged");
            Ok(KalGetResponse::Status304_NotModified {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            })
        } else {
            tx.commit().await?;
            let prp = &result.0;
            info!("{} Sitzungen retrieved", result.1.len());
            Ok(KalGetResponse::Status200_SuccessfulResponse {
                body: result.1,
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
                x_total_count: Some(prp.x_total_count),
                x_total_pages: Some(prp.x_total_pages),
                x_page: Some(prp.x_page),
                x_per_page: Some(prp.x_per_page),
                link: Some(prp.generate_link_header("/api/v2/kalender")),
            })
        }
    }

    #[doc = "SGetById - GET /api/v2/sitzung/{sid}"]
    #[instrument(skip_all, fields(sid=%path_params.sid))]
    async fn s_get_by_id(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        header_params: &models::SGetByIdHeaderParams,
        path_params: &models::SGetByIdPathParams,
    ) -> Result<SGetByIdResponse> {
        // TODO: find a better way to handle admin-only info for every object
        // in the past this was done by optional authentication, but that
        // turned out not to neatly fit into the oapi spec.
        // for now this is just a disabled feature
        let claims = (APIScope::Collector, 0);
        let mut tx = self.sqlx_db.begin().await?;
        let api_id = path_params.sid;
        let id_exists = sqlx::query!("SELECT 1 as x FROM sitzung WHERE api_id = $1", api_id)
            .fetch_optional(&mut *tx)
            .await?;
        if id_exists.is_none() {
            info!("Sitzung does not exist");
            return Ok(SGetByIdResponse::Status404_NotFound {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }

        let id = sqlx::query!(
            "SELECT id FROM sitzung WHERE api_id = $1
        AND last_update > COALESCE($2::timestamptz, '1940-01-01T00:00:00');",
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
            info!("Success");
            Ok(SGetByIdResponse::Status200_Success {
                body: result,
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            })
        } else if let Some(ims) = header_params.if_modified_since {
            info!("Success, but not modified since {}", ims);
            Ok(SGetByIdResponse::Status304_NotModified {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            })
        } else {
            error!("Session ID was not found a second time despite if_modified_since being None. This might indicate a grave database state error.\n
            Call Parameters: {:?}, {:?}", path_params, header_params);
            unreachable!("This should not happen.")
        }
    }

    #[doc = "SGet - GET /api/v2/sitzung"]
    #[instrument(skip_all)]
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
            warn!(
                "Parameters were chosen such that the request is unsatisfiable: {:?}, ims={:?}",
                query_params, header_params.if_modified_since
            );
            return Ok(SGetResponse::Status416_RequestRangeNotSatisfiable {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let params = retrieve::SitzungFilterParameters {
            gremium_like: query_params.gr.clone(),
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
            info!("No Content found matching the date criteria");
            Ok(SGetResponse::Status204_NoContent {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            })
        } else if let Some(ims) = header_params.if_modified_since
            && result.1.is_empty()
        {
            info!("No Content found that was modified since {}", ims);
            Ok(SGetResponse::Status304_NotModified {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            })
        } else {
            info!("Successfully retrieved {} Sitzungen", result.1.len());
            Ok(SGetResponse::Status200_SuccessfulResponse {
                body: result.1,
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
                x_total_count: Some(prp.x_total_count),
                x_total_pages: Some(prp.x_total_pages),
                x_page: Some(prp.x_page),
                x_per_page: Some(prp.x_per_page),
                link: Some(prp.generate_link_header("/api/v2/sitzung")),
            })
        }
    }
}

#[cfg(test)]
mod sitzung_test {
    use axum::http::Method;
    use axum_extra::extract::CookieJar;
    use chrono::Utc;
    use openapi::apis::collector_schnittstellen_sitzung::*;
    use openapi::apis::data_administration_sitzung::*;
    use openapi::apis::sitzung_unauthorisiert::*;
    use openapi::models::KalDateGetHeaderParams;
    use openapi::models::KalDateGetPathParams;
    use openapi::models::KalDateGetQueryParams;
    use openapi::models::SidPutPathParams;
    use tracing::info;
    use tracing_test::traced_test;

    use chrono::Datelike;
    use openapi::models;
    use uuid::Uuid;

    use crate::api::RoundTimestamp;

    use crate::utils::testing::{TestSetup, generate};

    use super::super::auth;

    fn localhost() -> headers::Host {
        use http::uri::Authority;
        Authority::from_static("localhost").into()
    }

    // Calendar tests
    #[tokio::test]
    async fn test_calendar_auth() {
        let scenario = TestSetup::new("test_calendar_auth").await;
        let server = &scenario.server;
        let host = localhost();
        let cookies = CookieJar::new();
        let test_date = chrono::Utc::now().date_naive();
        let test_session = generate::default_sitzung();

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
                &vec![test_session],
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

    #[tokio::test]
    #[traced_test]
    async fn test_cal_date_put() {
        // Setup test server and database
        let scenario = TestSetup::new("test_cal_date_put").await;
        let server = &scenario.server;
        let host = localhost();
        let cookies = CookieJar::new();

        // Create test calendar entry
        let today = chrono::Utc::now().date_naive();
        let outdated_session = generate::default_sitzung();
        let recent_session = models::Sitzung {
            termin: chrono::Utc::now(),
            ..outdated_session.clone()
        };
        let parlament = recent_session.gremium.parlament;

        // Test cases for kal_date_put:
        // 1. Create calendar entry out of valid date range with admin permissions
        // result: should not be possible, the invalid data points are silently filtered out
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
                    datum: today,
                    parlament,
                },
                &vec![outdated_session.clone()],
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

        // 2. Update calendar entry with date constraints and fail (collector is only allowed to update recent dates)
        // this is rejected with forbidden
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
                    datum: today,
                    parlament,
                },
                &vec![outdated_session],
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
        // Update calendar entry with date constraints and succeed
        // this is accepted and an entry is created
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
                    datum: today,
                    parlament,
                },
                &vec![recent_session],
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
        let response = server
            .kal_date_get(
                &Method::PUT,
                &host,
                &cookies,
                &KalDateGetHeaderParams {
                    if_modified_since: None,
                },
                &KalDateGetPathParams {
                    datum: today,
                    parlament,
                },
                &KalDateGetQueryParams {
                    page: None,
                    per_page: None,
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            response,
            KalDateGetResponse::Status200_SuccessfulResponse { body, .. }
            if body.len() == 1
        ));
        scenario.teardown().await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_kal_date_get() {
        let setup = TestSetup::new("kal_date_get").await;
        let server = &setup.server;
        let host = localhost();
        let cookies = CookieJar::new();
        let old_session = generate::default_sitzung();
        let session = models::Sitzung {
            termin: chrono::Utc::now(),
            ..old_session
        };
        let parlament = session.gremium.parlament;
        let today = session.termin.date_naive();

        let _response = server
            .kal_date_put(
                &Method::PUT,
                &host,
                &cookies,
                &(auth::APIScope::Collector, 1),
                &models::KalDatePutHeaderParams {
                    x_scraper_id: Uuid::nil(),
                },
                &models::KalDatePutPathParams {
                    datum: session.termin.date_naive(),
                    parlament,
                },
                &vec![session.clone()],
            )
            .await
            .unwrap();
        // 2. Get calendar entry for non-existent date
        let response = server
            .kal_date_get(
                &Method::GET,
                &host,
                &cookies,
                &models::KalDateGetHeaderParams {
                    if_modified_since: None,
                },
                &models::KalDateGetPathParams {
                    datum: chrono::NaiveDate::from_ymd_opt(1950, 10, 10).unwrap(),
                    parlament,
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
        // successful GET
        let response = server
            .kal_date_get(
                &Method::GET,
                &host,
                &cookies,
                &models::KalDateGetHeaderParams {
                    if_modified_since: None,
                },
                &models::KalDateGetPathParams {
                    datum: today,
                    parlament,
                },
                &models::KalDateGetQueryParams {
                    page: None,
                    per_page: None,
                },
            )
            .await
            .unwrap();
        let normalized_session = super::st_to_uuiddoks(&session).with_round_timestamps();
        info!("expected: {:?}", vec![normalized_session.clone()]);
        if let KalDateGetResponse::Status200_SuccessfulResponse { ref body, .. } = response {
            info!("actual  : {:?}", body)
        }
        assert!(matches!(
            response,
            KalDateGetResponse::Status200_SuccessfulResponse { body, .. }
            if body.iter().map(|b| b.with_round_timestamps()).collect::<Vec<_>>() == vec![normalized_session]
        ));
        // see if if-mod-since works
        let response = server
            .kal_date_get(
                &Method::GET,
                &host,
                &cookies,
                &models::KalDateGetHeaderParams {
                    if_modified_since: Some(chrono::Utc::now()),
                },
                &models::KalDateGetPathParams {
                    datum: today,
                    parlament,
                },
                &models::KalDateGetQueryParams {
                    page: None,
                    per_page: None,
                },
            )
            .await
            .unwrap();
        assert!(
            matches!(response, KalDateGetResponse::Status404_NotFound { .. },),
            "Expected 404, got {:?}",
            response
        );
        // Cleanup
        setup.teardown().await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_kal_get() {
        let setup = TestSetup::new("kal_get").await;
        let server = &setup.server;
        let host = localhost();
        let cookies = CookieJar::new();
        let session = generate::default_sitzung();
        let parlament = session.gremium.parlament;
        let date = session.termin;

        let response = server
            .sid_put(
                &Method::PUT,
                &host,
                &cookies,
                &(auth::APIScope::Admin, 1),
                &SidPutPathParams { sid: Uuid::nil() },
                &session.clone(),
            )
            .await
            .unwrap();
        assert!(matches!(response, SidPutResponse::Status201_Created { .. }));

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
                        y: Some(date.year()),
                        m: Some(date.month0() as i32 + 1),
                        dom: None,
                        gr: None,
                        p: Some(parlament),
                        since: None,
                        until: None,
                        wp: Some(session.gremium.wahlperiode as i32),
                    },
                )
                .await
                .unwrap();
            match &response {
                KalGetResponse::Status200_SuccessfulResponse { body, .. } => {
                    assert!(
                        !body.is_empty(),
                        "Expected 204 no content, got 200 OK with empty body"
                    );
                }
                _ => panic!(
                    "Expected to find sessions with valid filters, got: {:?}",
                    &response
                ),
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
            let start_date = session
                .termin
                .checked_sub_days(chrono::Days::new(1))
                .unwrap();
            let end_date = session
                .termin
                .checked_add_days(chrono::Days::new(1))
                .unwrap();
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
                        p: Some(parlament),
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

        let response = server
            .kal_get(
                &Method::GET,
                &host,
                &cookies,
                &models::KalGetHeaderParams {
                    if_modified_since: Some(
                        chrono::Utc::now()
                            .checked_add_days(chrono::Days::new(1))
                            .unwrap(),
                    ),
                },
                &models::KalGetQueryParams {
                    page: None,
                    per_page: None,
                    y: None,
                    m: None,
                    dom: None,
                    gr: None,
                    p: Some(parlament),
                    since: None,
                    until: None,
                    wp: None,
                },
            )
            .await
            .unwrap();
        assert!(
            matches!(response, KalGetResponse::Status304_NotModified { .. }),
            "{:?}",
            response
        );
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
                    p: Some(parlament),
                    since: Some(
                        chrono::Utc::now()
                            .checked_add_days(chrono::Days::new(1))
                            .unwrap(),
                    ),
                    until: None,
                    wp: None,
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            response,
            KalGetResponse::Status204_NoContent { .. }
        ));
        setup.teardown().await;
    }

    #[tokio::test]
    pub(crate) async fn test_session_get_endpoints() {
        // Setup test server and database
        let scenario = TestSetup::new("test_session_get").await;
        let server = &scenario.server;

        let test_session = generate::default_sitzung();
        // First create the session
        let create_response = server
            .sid_put(
                &Method::PUT,
                &localhost(),
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
                    &localhost(),
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
                    &localhost(),
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
                    &localhost(),
                    &CookieJar::new(),
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
                    &localhost(),
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
                    &localhost(),
                    &CookieJar::new(),
                    &models::SGetHeaderParams {
                        if_modified_since: None,
                    },
                    &models::SGetQueryParams {
                        page: None,
                        gr: None,
                        per_page: None,
                        p: Some(test_session.gremium.parlament),
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
                rsp => panic!("Expected successful operation response, got {rsp:?}"),
            }
        }

        // 2. Get sessions with invalid parameters
        {
            let response = server
                .s_get(
                    &Method::GET,
                    &localhost(),
                    &CookieJar::new(),
                    &models::SGetHeaderParams {
                        if_modified_since: None,
                    },
                    &models::SGetQueryParams {
                        page: None,
                        per_page: None,
                        gr: None,
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

        // 3. Get sessions with filters
        {
            let response = server
                .s_get(
                    &Method::GET,
                    &localhost(),
                    &CookieJar::new(),
                    &models::SGetHeaderParams {
                        if_modified_since: None,
                    },
                    &models::SGetQueryParams {
                        page: None,
                        per_page: None,
                        p: Some(test_session.gremium.parlament),
                        since: None,
                        gr: None,
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
                    &localhost(),
                    &CookieJar::new(),
                    &models::SGetHeaderParams {
                        if_modified_since: Some(chrono::Utc::now()),
                    },
                    &models::SGetQueryParams {
                        page: None,
                        per_page: None,
                        p: Some(models::Parlament::Bt),
                        since: None,
                        gr: None,
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
                    &localhost(),
                    &CookieJar::new(),
                    &models::SGetHeaderParams {
                        if_modified_since: None,
                    },
                    &models::SGetQueryParams {
                        page: None,
                        gr: None,
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
    #[traced_test]
    async fn test_session_modify_endpoints() {
        let scenario = TestSetup::new("session_modify_ep").await;
        let server = &scenario.server;
        let sitzung = generate::random::sitzung(12);
        // - Input non-existing session
        {
            let response = server
                .sid_put(
                    &Method::PUT,
                    &localhost(),
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
        }

        // - Update existing session with the same data
        let response = server
            .sid_put(
                &Method::PUT,
                &localhost(),
                &CookieJar::new(),
                &(auth::APIScope::Admin, 1),
                &models::SidPutPathParams {
                    sid: sitzung.api_id.unwrap(),
                },
                &sitzung,
            )
            .await
            .unwrap();
        assert!(
            matches!(response, SidPutResponse::Status304_NotModified { .. }),
            "Failed to update existing session with the same data.\nInput:  {:?}\n\n Output: {:?}",
            sitzung,
            response,
        );

        // - Update existing session with valid new data
        let rsp_new = models::Sitzung {
            link: Some("https://example.com/a/b/c".to_string()),
            ..sitzung.clone()
        };
        let response = server
            .sid_put(
                &Method::PUT,
                &localhost(),
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
                &localhost(),
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
                    &localhost(),
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
                    &localhost(),
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
                    &localhost(),
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

    #[tokio::test]
    async fn test_malformed_req_data() {
        // dokumente uniqueness konflikt
    }
}
