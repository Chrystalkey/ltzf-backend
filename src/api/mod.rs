use chrono::DurationRound;
use std::cmp::Ordering;
use std::fmt::Debug;
use std::fmt::Display;
use std::sync::Arc;

use async_trait::async_trait;
use axum_extra::extract::Host;
use openapi::models;
use tracing::debug;
use tracing::instrument;

use crate::Configuration;
use crate::Result;
use crate::error::LTZFError;
use crate::utils::notify;
use openapi::apis::unauthorisiert::*;

pub(crate) mod auth;
pub(crate) mod misc;
pub(crate) mod misc_auth;
pub(crate) mod sitzung;
pub(crate) mod vorgang;

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
    #[instrument(skip_all, fields(%method))]
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

#[async_trait]
impl Unauthorisiert<LTZFError> for LTZFServer {
    #[instrument(skip_all, fields(t=?query_params.t))]
    async fn ping(
        &self,
        _method: &axum::http::Method,
        _host: &Host,
        _cookies: &axum_extra::extract::CookieJar,
        query_params: &models::PingQueryParams,
    ) -> Result<PingResponse> {
        if let Some(t) = query_params.t {
            let current_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs_f64();
            debug!("Ping with one-way time: {} s", current_time - t);
        }
        Ok(PingResponse::Status200_Pong)
    }

    #[instrument(skip_all)]
    async fn status(
        &self,
        _method: &axum::http::Method,
        _host: &Host,
        _cookies: &axum_extra::extract::CookieJar,
    ) -> Result<StatusResponse> {
        debug!("Status Requested");
        // TODO: implement "API is not running for some reason" markers
        Ok(StatusResponse::Status200_APIIsRunning {
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
        })
    }
}
#[derive(Debug)]
pub struct PaginationResponsePart {
    pub x_total_count: i32,
    pub x_total_pages: i32,
    pub x_page: i32,
    pub x_per_page: i32,
}
impl PaginationResponsePart {
    pub const DEFAULT_PER_PAGE: i32 = 32;
    pub const MAX_PER_PAGE: i32 = 256;
    pub fn new(x_total_count: i32, x_page: Option<i32>, x_per_page: Option<i32>) -> Self {
        let x_per_page = x_per_page
            .map(|x| x.clamp(0, Self::MAX_PER_PAGE))
            .unwrap_or(Self::DEFAULT_PER_PAGE);
        let x_total_pages = ((x_total_count as f32) / x_per_page as f32).ceil().max(1.) as i32;
        let x_page = x_page.map(|x| x.clamp(1, x_total_pages)).unwrap_or(1);

        Self {
            x_total_count,
            x_total_pages,
            x_page,
            x_per_page,
        }
    }
    pub fn limit(&self) -> i64 {
        self.x_per_page as i64
    }
    pub fn offset(&self) -> i64 {
        ((self.x_page - 1) * self.x_per_page) as i64
    }
    pub fn start(&self) -> usize {
        ((self.x_page - 1) * self.x_per_page) as usize
    }
    pub fn end(&self) -> usize {
        (self.offset() + self.limit())
            .min(self.x_total_count as i64)
            .max(0) as usize
    }
    pub fn generate_link_header(&self, link_first_part: &str) -> String {
        let mut link_string = String::new();
        if self.x_page < self.x_total_pages {
            link_string = format!(
                "<\"{}?page={}&per_page={}\">; rel=\"next\", ",
                link_first_part,
                self.x_page + 1,
                self.x_per_page
            );
        }
        if self.x_page > 1 {
            link_string = format!(
                "{}<\"{}?page={}&per_page={}\">; rel=\"previous\", ",
                link_string,
                link_first_part,
                self.x_page - 1,
                self.x_per_page
            );
        }
        link_string = format!(
            "{}<\"{}?page={}&per_page={}\">; rel=\"first\", ",
            link_string, link_first_part, 1, self.x_per_page
        );
        link_string = format!(
            "{}<\"{}?page={}&per_page={}\">; rel=\"last\"",
            link_string,
            link_first_part,
            self.x_total_pages.max(1),
            self.x_per_page
        );
        link_string
    }
}

#[cfg(test)]
mod prp_test {
    use crate::api::PaginationResponsePart;
    #[test]
    fn test_link_header() {
        let prp = PaginationResponsePart::new(0, None, Some(16));
        let lh = prp.generate_link_header("/");
        let link_hdr_parts: Vec<_> = lh.split(", ").collect();
        assert!(
            link_hdr_parts
                .iter()
                .any(|x| *x == "<\"/?page=1&per_page=16\">; rel=\"first\""),
            "{:?}",
            link_hdr_parts
        );
        assert!(
            link_hdr_parts
                .iter()
                .any(|x| *x == "<\"/?page=1&per_page=16\">; rel=\"last\""),
            "{:?}",
            link_hdr_parts
        );
        assert_eq!(link_hdr_parts.len(), 2);

        let prp = PaginationResponsePart::new(100, Some(1), Some(16));
        let lh = prp.generate_link_header("/");
        let link_hdr_parts: Vec<_> = lh.split(", ").collect();
        assert!(
            link_hdr_parts
                .iter()
                .any(|x| *x == "<\"/?page=2&per_page=16\">; rel=\"next\""),
            "{:?}",
            link_hdr_parts
        );
        assert!(
            link_hdr_parts
                .iter()
                .any(|x| *x == "<\"/?page=1&per_page=16\">; rel=\"first\""),
            "{:?}",
            link_hdr_parts
        );
        assert!(
            link_hdr_parts
                .iter()
                .any(|x| *x == "<\"/?page=7&per_page=16\">; rel=\"last\""),
            "{:?}",
            link_hdr_parts
        );
        assert_eq!(link_hdr_parts.len(), 3);

        let prp = PaginationResponsePart::new(100, Some(2), Some(16));
        let lh = prp.generate_link_header("/");
        let link_hdr_parts: Vec<_> = lh.split(", ").collect();
        assert!(
            link_hdr_parts
                .iter()
                .any(|x| *x == "<\"/?page=3&per_page=16\">; rel=\"next\""),
            "{:?}",
            link_hdr_parts
        );
        assert!(
            link_hdr_parts
                .iter()
                .any(|x| *x == "<\"/?page=1&per_page=16\">; rel=\"previous\""),
            "{:?}",
            link_hdr_parts
        );
        assert!(
            link_hdr_parts
                .iter()
                .any(|x| *x == "<\"/?page=1&per_page=16\">; rel=\"first\""),
            "{:?}",
            link_hdr_parts
        );
        assert!(
            link_hdr_parts
                .iter()
                .any(|x| *x == "<\"/?page=7&per_page=16\">; rel=\"last\""),
            "{:?}",
            link_hdr_parts
        );
        assert_eq!(link_hdr_parts.len(), 4);
    }

    #[test]
    fn test_start_and_end() {
        let prp = PaginationResponsePart::new(0, None, None);
        assert_eq!(prp.start(), 0);
        assert_eq!(prp.end(), 0, "{prp:?}");

        let prp = PaginationResponsePart::new(1, None, None);
        assert_eq!(prp.start(), 0);
        assert_eq!(prp.end(), 1);
    }
}

pub struct DateRange {
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub until: Option<chrono::DateTime<chrono::Utc>>,
}

impl Debug for DateRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}
impl Display for DateRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}, {}]",
            self.since
                .map(|x| format!("{}", x))
                .unwrap_or("-∞".to_string()),
            self.until
                .map(|x| format!("{}", x))
                .unwrap_or("∞".to_string())
        )
    }
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
    if let Some(since) = since {
        if since_min.is_some() {
            since_min = Some(since_min.unwrap().min(since));
        } else {
            since_min = Some(since);
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

#[cfg(test)]
mod test_applicable_date_range {
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
        let result = find_applicable_date_range(Some(y), Some(m), Some(d), None, None, None);
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
        let result = find_applicable_date_range(Some(y), Some(m), None, None, None, None);
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
        let result = find_applicable_date_range(Some(y), None, None, None, None, None);
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

/// this is here to implement a PartialEq, Eq, Ord, ...
/// for hashing, since we need t
#[derive(Debug, Clone)]
pub(crate) struct WrappedAutor<'wrapped> {
    pub autor: &'wrapped models::Autor,
}
impl<'wrapped> PartialEq for WrappedAutor<'wrapped> {
    fn eq(&self, other: &Self) -> bool {
        self.autor.organisation == other.autor.organisation
            && self.autor.person == other.autor.person
    }
}
impl<'wrapped> Eq for WrappedAutor<'wrapped> {}
impl<'wrapped> PartialOrd for WrappedAutor<'wrapped> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(std::cmp::Ord::cmp(self, other))
    }
}
impl<'wrapped> Ord for WrappedAutor<'wrapped> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.autor
            .organisation
            .cmp(&other.autor.organisation)
            .then(self.autor.person.cmp(&other.autor.person))
    }
}
/// This trait enables sorting all arrays contained in an object
/// to be able to compare them afterwards without caring for ordering
#[cfg(test)]
pub(crate) trait SortArrays: Clone {
    fn sort_arrays(&mut self);
}

#[cfg(test)]
impl SortArrays for models::Dokument {
    fn sort_arrays(&mut self) {
        let nil = uuid::Uuid::nil();
        let emp = "".to_owned();
        if let Some(x) = self.schlagworte.as_mut() {
            x.sort();
        }
        if let Some(x) = self.touched_by.as_mut() {
            x.sort_by(|a, b| {
                (a.key.as_ref().unwrap_or(&emp), a.scraper_id.unwrap_or(nil))
                    .cmp(&(b.key.as_ref().unwrap_or(&emp), b.scraper_id.unwrap_or(nil)))
            })
        }
        self.autoren
            .sort_by(|a, b| a.organisation.cmp(&b.organisation));
    }
}
#[cfg(test)]
impl SortArrays for models::Station {
    fn sort_arrays(&mut self) {
        let nil = uuid::Uuid::nil();
        let emp = "".to_owned();
        if let Some(x) = self.schlagworte.as_mut() {
            x.sort();
        }
        if let Some(x) = self.touched_by.as_mut() {
            x.sort_by(|a, b| {
                (a.key.as_ref().unwrap_or(&emp), a.scraper_id.unwrap_or(nil))
                    .cmp(&(b.key.as_ref().unwrap_or(&emp), b.scraper_id.unwrap_or(nil)))
            })
        }
        if let Some(x) = self.additional_links.as_mut() {
            x.sort();
        }
        self.dokumente.sort_by(|a, b| match (a, b) {
            (
                models::StationDokumenteInner::String(x),
                models::StationDokumenteInner::String(y),
            ) => x.cmp(y),
            (
                models::StationDokumenteInner::String(x),
                models::StationDokumenteInner::Dokument(y),
            ) => (**x).cmp(&y.api_id.unwrap_or(nil).to_string()),
            (
                models::StationDokumenteInner::Dokument(x),
                models::StationDokumenteInner::String(y),
            ) => (**y).cmp(&x.api_id.unwrap_or(nil).to_string()),
            (
                models::StationDokumenteInner::Dokument(x),
                models::StationDokumenteInner::Dokument(y),
            ) => x.api_id.unwrap_or(nil).cmp(&y.api_id.unwrap_or(nil)),
        });
        self.dokumente.iter_mut().for_each(|x| {
            if let models::StationDokumenteInner::Dokument(x) = x {
                x.sort_arrays();
            }
        });
        if let Some(x) = self.stellungnahmen.as_mut() {
            x.sort_by(|a, b| match (a, b) {
                (
                    models::StationDokumenteInner::String(x),
                    models::StationDokumenteInner::String(y),
                ) => x.cmp(y),
                (
                    models::StationDokumenteInner::String(x),
                    models::StationDokumenteInner::Dokument(y),
                ) => (**x).cmp(&y.api_id.unwrap_or(nil).to_string()),
                (
                    models::StationDokumenteInner::Dokument(x),
                    models::StationDokumenteInner::String(y),
                ) => (**y).cmp(&x.api_id.unwrap_or(nil).to_string()),
                (
                    models::StationDokumenteInner::Dokument(x),
                    models::StationDokumenteInner::Dokument(y),
                ) => x.api_id.unwrap_or(nil).cmp(&y.api_id.unwrap_or(nil)),
            });
            x.iter_mut().for_each(|x| {
                if let models::StationDokumenteInner::Dokument(x) = x {
                    x.sort_arrays();
                }
            });
        }
    }
}
#[cfg(test)]
impl SortArrays for models::Vorgang {
    fn sort_arrays(&mut self) {
        let nil = uuid::Uuid::nil();
        let emp = "".to_owned();
        if let Some(x) = self.touched_by.as_mut() {
            x.sort_by(|a, b| {
                (a.key.as_ref().unwrap_or(&emp), a.scraper_id.unwrap_or(nil))
                    .cmp(&(b.key.as_ref().unwrap_or(&emp), b.scraper_id.unwrap_or(nil)))
            })
        }
        if let Some(x) = self.lobbyregister.as_mut() {
            x.sort_by(|a, b| a.link.cmp(&b.link));
        }
        self.stationen
            .sort_by(|a, b| (a.zp_start, a.api_id).cmp(&(b.zp_start, b.api_id)));
        self.stationen.iter_mut().for_each(|a| a.sort_arrays());
        self.initiatoren
            .sort_by(|a, b| a.organisation.cmp(&b.organisation));
        if let Some(x) = self.ids.as_mut() {
            x.sort_by(|a, b| (a.typ, &a.id).cmp(&(b.typ, &b.id)));
        }
        if let Some(x) = self.links.as_mut() {
            x.sort();
        }
    }
}
#[cfg(test)]
impl SortArrays for models::Sitzung {
    fn sort_arrays(&mut self) {
        let nil = uuid::Uuid::nil();
        let emp = "".to_owned();
        if let Some(x) = self.touched_by.as_mut() {
            x.sort_by(|a, b| {
                (a.key.as_ref().unwrap_or(&emp), a.scraper_id.unwrap_or(nil))
                    .cmp(&(b.key.as_ref().unwrap_or(&emp), b.scraper_id.unwrap_or(nil)))
            })
        }
        if let Some(x) = self.experten.as_mut() {
            x.sort_by(|a, b| a.organisation.cmp(&b.organisation));
        }

        if let Some(x) = self.dokumente.as_mut() {
            x.sort_by(|a, b| match (a, b) {
                (
                    models::StationDokumenteInner::String(x),
                    models::StationDokumenteInner::String(y),
                ) => x.cmp(y),
                (
                    models::StationDokumenteInner::String(x),
                    models::StationDokumenteInner::Dokument(y),
                ) => (**x).cmp(&y.api_id.unwrap_or(nil).to_string()),
                (
                    models::StationDokumenteInner::Dokument(x),
                    models::StationDokumenteInner::String(y),
                ) => (**y).cmp(&x.api_id.unwrap_or(nil).to_string()),
                (
                    models::StationDokumenteInner::Dokument(x),
                    models::StationDokumenteInner::Dokument(y),
                ) => x.api_id.unwrap_or(nil).cmp(&y.api_id.unwrap_or(nil)),
            });

            x.iter_mut().for_each(|x| {
                if let models::StationDokumenteInner::Dokument(x) = x {
                    x.sort_arrays();
                }
            });
        }
        self.tops.sort_by(|a, b| a.nummer.cmp(&b.nummer));
    }
}
/// Helper Trait that allows me to compare objects (vorgang, dokument, ...)
/// that are stored and re-fetched with whatever precision where only the
/// very very margins differ by a few nanosecs.
/// Using this trait the thing can round its dates to a precision of 1 second
/// We do not really need more
pub(crate) trait RoundTimestamp: Clone {
    fn with_round_timestamps(&self) -> Self;
}

impl RoundTimestamp for models::Dokument {
    fn with_round_timestamps(&self) -> Self {
        let precision = chrono::Duration::seconds(1);

        Self {
            zp_referenz: self.zp_referenz.duration_round(precision).unwrap(),
            zp_erstellt: self
                .zp_erstellt
                .map(|ts| ts.duration_round(precision).unwrap()),
            zp_modifiziert: self.zp_modifiziert.duration_round(precision).unwrap(),
            ..self.clone()
        }
    }
}
impl RoundTimestamp for models::Station {
    fn with_round_timestamps(&self) -> Self {
        let precision = chrono::Duration::seconds(1);
        Self {
            zp_modifiziert: self
                .zp_modifiziert
                .map(|ts| ts.duration_round(precision).unwrap()),
            zp_start: self.zp_start.duration_round(precision).unwrap(),
            stellungnahmen: self.stellungnahmen.as_ref().map(|v| {
                v.iter()
                    .map(|sn| match sn {
                        models::StationDokumenteInner::Dokument(d) => {
                            models::StationDokumenteInner::Dokument(Box::new(
                                d.with_round_timestamps(),
                            ))
                        }
                        x => x.clone(),
                    })
                    .collect()
            }),
            dokumente: self
                .dokumente
                .iter()
                .map(|sn| match sn {
                    models::StationDokumenteInner::Dokument(d) => {
                        models::StationDokumenteInner::Dokument(Box::new(d.with_round_timestamps()))
                    }
                    x => x.clone(),
                })
                .collect(),

            ..self.clone()
        }
    }
}

impl RoundTimestamp for models::Vorgang {
    fn with_round_timestamps(&self) -> Self {
        Self {
            stationen: self
                .stationen
                .iter()
                .map(|s| s.with_round_timestamps())
                .collect(),
            ..self.clone()
        }
    }
}
impl RoundTimestamp for models::Sitzung {
    fn with_round_timestamps(&self) -> Self {
        let precision = chrono::Duration::seconds(1);
        Self {
            dokumente: self.dokumente.as_ref().map(|v| {
                v.iter()
                    .map(|sn| match sn {
                        models::StationDokumenteInner::Dokument(d) => {
                            models::StationDokumenteInner::Dokument(Box::new(
                                d.with_round_timestamps(),
                            ))
                        }
                        x => x.clone(),
                    })
                    .collect()
            }),
            termin: self.termin.duration_round(precision).unwrap(),
            ..self.clone()
        }
    }
}
