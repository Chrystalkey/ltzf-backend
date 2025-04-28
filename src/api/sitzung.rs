use crate::db::{delete, insert, retrieve};
use crate::error::LTZFError;
use crate::{LTZFServer, Result};
use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::{CookieJar, Host};
use openapi::apis::adminschnittstellen_collector_schnittstellen_kalender_sitzungen::*;
use openapi::apis::adminschnittstellen_sitzungen::*;
use openapi::apis::kalender_sitzungen_unauthorisiert::*;
use openapi::apis::sitzungen_unauthorisiert::*;
use openapi::models;
use openapi::models::SGetQueryParams;
use openapi::models::*;
use sqlx::PgTransaction;

use super::auth::{self, APIScope};
use super::compare::*;
#[async_trait]
impl AdminschnittstellenSitzungen<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;
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
            return Ok(SitzungDeleteResponse::Status403_AuthenticationFailed {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        Ok(delete::delete_sitzung_by_api_id(path_params.sid, self).await?)
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
            return Ok(SidPutResponse::Status403_AuthenticationFailed {
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
                    insert::insert_sitzung(body, &mut tx, self).await?;
                }
                _ => {
                    unreachable!("If this is reached, some assumptions did not hold")
                }
            }
        } else {
            insert::insert_sitzung(body, &mut tx, self).await?;
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
impl KalenderSitzungenUnauthorisiert<LTZFError> for LTZFServer {
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
        query_params: &models::KalDateGetQueryParams,
    ) -> Result<KalDateGetResponse> {
        let mut tx = self.sqlx_db.begin().await?;
        let date = path_params.datum;
        let dt_begin = date
            .and_time(chrono::NaiveTime::from_hms_micro_opt(0, 0, 0, 0).unwrap())
            .and_utc();
        let dt_end = date
            .checked_add_days(chrono::Days::new(1))
            .unwrap()
            .and_time(chrono::NaiveTime::from_hms_micro_opt(0, 0, 0, 0).unwrap())
            .and_utc();
        let sids = sqlx::query!(
            "SELECT s.id FROM sitzung s 
        INNER JOIN gremium g ON g.id = s.gr_id
        INNER JOIN parlament p ON p.id = g.parl 
        WHERE termin BETWEEN $1 AND $2 AND p.value = $3",
            dt_begin,
            dt_end,
            path_params.parlament.to_string()
        )
        .map(|r| r.id)
        .fetch_all(&mut *tx)
        .await?;
        if sids.is_empty() {
            return Ok(KalDateGetResponse::Status404_NotFound {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let mut vector = vec![];
        for sid in sids {
            vector.push(retrieve::sitzung_by_id(sid, &mut tx).await?);
        }
        tx.commit().await?;
        Ok(KalDateGetResponse::Status200_SuccessfulResponseContainingAListOfParliamentarySessionsMatchingTheQueryFilters 
            { body: vector, x_rate_limit_limit: None, x_rate_limit_remaining: None, x_rate_limit_reset: None, x_total_count: (), x_total_pages: (), x_page: (), x_per_page: () })
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
        let res = sitzung::kal_get_by_param(query_params, header_params, &mut tx, self).await?;
        tx.commit().await?;
        Ok(res)
    }
}

#[async_trait]
impl SitzungenUnauthorisiert<LTZFError> for LTZFServer {
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

#[async_trait]
impl AdminschnittstellenCollectorSchnittstellenKalenderSitzungen<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;

    #[doc = "KalDatePut - PUT /api/v1/kalender/{parlament}/{datum}"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn kal_date_put(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
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
            return Ok(KalDatePutResponse::Status403_AuthenticationFailed {
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

        let res = sitzung::kal_put_by_date(
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
}

pub async fn s_get_by_id(
    server: &LTZFServer,
    header_params: &models::SGetByIdHeaderParams,
    path_params: &models::SGetByIdPathParams,
) -> Result<openapi::apis::default::SGetByIdResponse> {
    use openapi::apis::default::SGetByIdResponse;
    let mut tx = server.sqlx_db.begin().await?;
    let api_id = path_params.sid;
    let id_exists = sqlx::query!("SELECT 1 as x FROM sitzung WHERE api_id = $1", api_id)
        .fetch_optional(&mut *tx)
        .await?;
    if id_exists.is_none() {
        return Ok(SGetByIdResponse::Status404_ContentNotFound);
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
        let result = retrieve::sitzung_by_id(id, &mut tx).await?;
        tx.commit().await?;
        Ok(SGetByIdResponse::Status200_SuccessfulOperation(result))
    } else if header_params.if_modified_since.is_some() {
        Ok(SGetByIdResponse::Status304_NotModified)
    } else {
        Err(crate::error::LTZFError::Validation {
            source: Box::new(crate::error::DataValidationError::QueryParametersNotSatisfied),
        })
    }
}

pub async fn s_get(
    server: &LTZFServer,
    qparams: &SGetQueryParams,
    header_params: &models::SGetHeaderParams,
) -> Result<openapi::apis::default::SGetResponse> {
    let range = find_applicable_date_range(
        None,
        None,
        None,
        qparams.since,
        qparams.until,
        header_params.if_modified_since,
    );
    if range.is_none() {
        return Ok(openapi::apis::default::SGetResponse::Status416_RequestRangeNotSatisfiable);
    }
    let params = retrieve::SitzungFilterParameters {
        gremium_like: None,
        limit: qparams.limit.map(|x| x as u32),
        offset: qparams.offset.map(|x| x as u32),
        parlament: qparams.p,
        wp: qparams.wp.map(|x| x as u32),
        since: range.as_ref().unwrap().since,
        until: range.unwrap().until,
        vgid: qparams.vgid,
    };

    let mut tx: sqlx::Transaction<'_, sqlx::Postgres> = server.sqlx_db.begin().await?;
    let result = retrieve::sitzung_by_param(&params, &mut tx).await?;
    tx.commit().await?;
    if result.is_empty() && header_params.if_modified_since.is_none() {
        Ok(openapi::apis::default::SGetResponse::Status204_NoContentFoundForTheSpecifiedParameters)
    } else if result.is_empty() && header_params.if_modified_since.is_some() {
        Ok(openapi::apis::default::SGetResponse::Status304_NoNewChanges)
    } else {
        Ok(
        openapi::apis::default::SGetResponse::Status200_AntwortAufEineGefilterteAnfrageZuSitzungen(
            result,
        ),
    )
    }
}

/// expects valid input, does no further date-input validation
pub async fn kal_put_by_date(
    date: chrono::NaiveDate,
    parlament: Parlament,
    sessions: Vec<Sitzung>,
    tx: &mut PgTransaction<'_>,
    srv: &LTZFServer,
) -> Result<KalDatePutResponse> {
    let dt_begin = date
        .and_time(chrono::NaiveTime::from_hms_micro_opt(0, 0, 0, 0).unwrap())
        .and_utc();
    let dt_end = date
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
        parlament.to_string(),
        dt_begin,
        dt_end
    )
    .execute(&mut **tx)
    .await?;

    // insert all entries
    for s in &sessions {
        insert::insert_sitzung(s, tx, srv).await?;
    }
    Ok(KalDatePutResponse::Status201_Created)
}
pub struct DateRange {
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub until: Option<chrono::DateTime<chrono::Utc>>,
}
impl
    From<(
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    )> for DateRange
{
    fn from(
        value: (
            Option<chrono::DateTime<chrono::Utc>>,
            Option<chrono::DateTime<chrono::Utc>>,
        ),
    ) -> Self {
        Self {
            since: value.0,
            until: value.1,
        }
    }
}
pub fn find_applicable_date_range(
    y: Option<u32>,
    m: Option<u32>,
    d: Option<u32>,
    since: Option<chrono::DateTime<chrono::Utc>>,
    until: Option<chrono::DateTime<chrono::Utc>>,
    ifmodsince: Option<chrono::DateTime<chrono::Utc>>,
) -> Option<DateRange> {
    let ymd_date_range = if let Some(y) = y {
        if let Some(m) = m {
            if let Some(d) = d {
                Some((
                    chrono::NaiveDate::from_ymd_opt(y as i32, m, d).unwrap(),
                    chrono::NaiveDate::from_ymd_opt(y as i32, m, d).unwrap(),
                ))
            } else {
                Some((
                    chrono::NaiveDate::from_ymd_opt(y as i32, m, 1).unwrap(),
                    chrono::NaiveDate::from_ymd_opt(y as i32, m + 1, 1)
                        .unwrap()
                        .checked_sub_days(chrono::Days::new(1))
                        .unwrap(),
                ))
            }
        } else {
            Some((
                chrono::NaiveDate::from_ymd_opt(y as i32, 1, 1).unwrap(),
                chrono::NaiveDate::from_ymd_opt(y as i32, 12, 31).unwrap(),
            ))
        }
    } else {
        None
    }
    .map(|(a, b)| {
        (
            a.and_hms_opt(0, 0, 0).unwrap().and_utc(),
            b.and_hms_opt(23, 59, 59).unwrap().and_utc(),
        )
    });

    let mut since_min = ifmodsince;
    let mut until_min = until;
    if since.is_some() {
        if since_min.is_some() {
            since_min = Some(since_min.unwrap().min(since.unwrap()));
        } else {
            since_min = since;
        }
    }
    if let Some((ymd_s, ymd_u)) = ymd_date_range {
        if since_min.is_some() {
            since_min = Some(ymd_s.max(since_min.unwrap()));
        } else {
            since_min = Some(ymd_s);
        }
        if until_min.is_some() {
            until_min = Some(ymd_u.min(until_min.unwrap()));
        } else {
            until_min = Some(ymd_u);
        }
    }

    // semantic check
    if let Some(sm) = since_min {
        if sm < chrono::DateTime::parse_from_rfc3339("1945-01-01T00:00:00+00:00").unwrap() {
            return None;
        }
        if let Some(um) = until {
            if sm >= um {
                return None;
            }
        }
    }

    if let Some((ys, yu)) = ymd_date_range {
        if since_min.is_some() && since_min.unwrap() > yu
            || until_min.is_some() && until_min.unwrap() < ys
        {
            None
        } else {
            Some((since_min, until_min).into())
        }
    } else {
        Some((since_min, until_min).into())
    }
}

pub async fn kal_get_by_param(
    qparams: &KalGetQueryParams,
    hparams: &KalGetHeaderParams,
    tx: &mut PgTransaction<'_>,
    _srv: &LTZFServer,
) -> Result<KalGetResponse> {
    // input validation
    let result = find_applicable_date_range(
        qparams.y.map(|x| x as u32),
        qparams.m.map(|x| x as u32),
        qparams.dom.map(|x| x as u32),
        qparams.since,
        qparams.until,
        hparams.if_modified_since,
    );
    if result.is_none() {
        return Ok(KalGetResponse::Status416_RequestRangeNotSatisfiable);
    }

    let params = retrieve::SitzungFilterParameters {
        gremium_like: qparams.gr.clone(),
        limit: qparams.limit.map(|x| x as u32),
        offset: qparams.offset.map(|x| x as u32),
        parlament: qparams.p,
        vgid: None,
        wp: qparams.wp.map(|x| x as u32),
        since: result.as_ref().unwrap().since,
        until: result.unwrap().until,
    };

    // retrieval
    let result = retrieve::sitzung_by_param(&params, tx).await?;
    if result.is_empty() {
        Ok(KalGetResponse::Status204_NoContentFoundForTheSpecifiedParameters)
    } else {
        Ok(KalGetResponse::Status200_AntwortAufEineGefilterteAnfrageZuSitzungen(result))
    }
}

#[cfg(test)]
mod test {
    use super::find_applicable_date_range;
    use chrono::DateTime;

    #[test]
    fn test_date_range_none() {
        let result = find_applicable_date_range(None, None, None, None, None, None);
        assert!(
            result.is_some()
                && result.as_ref().unwrap().since.is_none()
                && result.unwrap().until.is_none(),
            "None dates should not fail but produce (None, None)"
        );
    }
    #[test]
    fn test_date_range_untilsince() {
        let since = DateTime::parse_from_rfc3339("1960-01-01T00:00:00+00:00")
            .unwrap()
            .to_utc();
        let until = DateTime::parse_from_rfc3339("1960-01-02T00:00:00+00:00")
            .unwrap()
            .to_utc();
        let result = find_applicable_date_range(None, None, None, Some(since), Some(until), None);
        assert!(
            result.is_some()
                && result.as_ref().unwrap().since == Some(since)
                && result.unwrap().until == Some(until),
            "Since and until should yield (since, until)"
        )
    }
    #[test]
    fn test_date_range_ymd() {
        let y = 2012u32;
        let m = 5u32;
        let d = 12u32;

        // ymd
        let result = find_applicable_date_range(
            Some(y as u32),
            Some(m as u32),
            Some(d as u32),
            None,
            None,
            None,
        );
        let expected_since = chrono::NaiveDate::from_ymd_opt(y as i32, m, d)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let expected_until = chrono::NaiveDate::from_ymd_opt(y as i32, m, d)
            .unwrap()
            .and_hms_opt(23, 59, 59)
            .unwrap()
            .and_utc();
        assert!(result.is_some());
        let result = result.unwrap();
        assert!(
            result.since == Some(expected_since) && result.until == Some(expected_until),
            "ymd should start and end at the date range"
        );
        // ym
        let result =
            find_applicable_date_range(Some(y as u32), Some(m as u32), None, None, None, None);
        let expected_since = chrono::NaiveDate::from_ymd_opt(y as i32, m, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let expected_until = chrono::NaiveDate::from_ymd_opt(y as i32, m, 31)
            .unwrap()
            .and_hms_opt(23, 59, 59)
            .unwrap()
            .and_utc();
        assert!(result.is_some());
        let result = result.unwrap();
        assert!(
            result.since == Some(expected_since) && result.until == Some(expected_until),
            "ymd should start and end at the date range"
        );
        // y
        let result = find_applicable_date_range(Some(y as u32), None, None, None, None, None);
        let expected_since = chrono::NaiveDate::from_ymd_opt(y as i32, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let expected_until = chrono::NaiveDate::from_ymd_opt(y as i32, 12, 31)
            .unwrap()
            .and_hms_opt(23, 59, 59)
            .unwrap()
            .and_utc();
        assert!(result.is_some());
        let result = result.unwrap();
        assert!(
            result.since == Some(expected_since) && result.until == Some(expected_until),
            "ymd should start and end at the date range"
        );
    }

    #[test]
    fn test_minmax() {
        let y = 2012u32;

        let since = chrono::NaiveDate::from_ymd_opt(2000, 3, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let until = chrono::NaiveDate::from_ymd_opt(2012, 7, 31)
            .unwrap()
            .and_hms_opt(15, 59, 59)
            .unwrap()
            .and_utc();

        let expected_since = chrono::NaiveDate::from_ymd_opt(2012, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let expected_until = chrono::NaiveDate::from_ymd_opt(2012, 7, 31)
            .unwrap()
            .and_hms_opt(15, 59, 59)
            .unwrap()
            .and_utc();

        let result =
            find_applicable_date_range(Some(y), None, None, Some(since), Some(until), None);
        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.since.is_some() && result.since.unwrap() == expected_since);
        assert!(result.until.is_some() && result.until.unwrap() == expected_until);
    }
}
