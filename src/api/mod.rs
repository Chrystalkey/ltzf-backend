use std::cmp::Ordering;
use std::sync::Arc;

use async_trait::async_trait;
use axum_extra::extract::Host;
use openapi::models;

use crate::Configuration;
use crate::Result;
use crate::error::LTZFError;
use crate::utils::notify;
use openapi::apis::unauthorisiert::*;

pub(crate) mod auth;
pub(crate) mod compare;
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
    async fn ping(
        &self,
        _method: &axum::http::Method,
        _host: &Host,
        _cookies: &axum_extra::extract::CookieJar,
    ) -> Result<PingResponse> {
        tracing::info!("Pong");
        Ok(PingResponse::Status200_Pong)
    }
    async fn status(
        &self,
        _method: &axum::http::Method,
        _host: &Host,
        _cookies: &axum_extra::extract::CookieJar,
    ) -> Result<StatusResponse> {
        tracing::info!("Status Requested");
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
        let x_total_pages = ((x_total_count as f32) / x_per_page as f32).ceil() as i32;
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
        if self.x_page != self.x_total_pages {
            link_string = format!(
                "<\"{}?page={}&per_page={}\">; rel=\"next\", ",
                link_first_part,
                self.x_page + 1,
                self.x_per_page
            );
        }
        if self.x_page != 0 {
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
            link_string, link_first_part, 0, self.x_per_page
        );
        link_string = format!(
            "{}<\"{}?page={}&per_page={}\">; rel=\"last\"",
            link_string, link_first_part, self.x_total_pages, self.x_per_page
        );
        link_string
    }
}

#[cfg(test)]
mod prp_test {
    use crate::api::PaginationResponsePart;

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

#[derive(Debug, Clone)]
pub(crate) struct WrappedAutor {
    pub autor: models::Autor,
}
impl PartialEq for WrappedAutor {
    fn eq(&self, other: &Self) -> bool {
        self.autor.organisation == other.autor.organisation
            && self.autor.person == other.autor.person
    }
}
impl Eq for WrappedAutor {}
impl PartialOrd for WrappedAutor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(std::cmp::Ord::cmp(self, other))
    }
}
impl Ord for WrappedAutor {
    fn cmp(&self, other: &Self) -> Ordering {
        self.autor
            .organisation
            .cmp(&other.autor.organisation)
            .then(self.autor.person.cmp(&other.autor.person))
    }
}

#[cfg(test)]
pub(crate) mod endpoint_test {
    use openapi::models;

    // Session (Sitzung) tests
    pub(crate) fn create_test_session() -> models::Sitzung {
        use chrono::Utc;
        use openapi::models::{
            Autor, Dokument, Gremium, Parlament, Sitzung, StationDokumenteInner, Top,
        };
        use uuid::Uuid;

        // Create a test document
        let test_doc = Dokument {
            touched_by: None,
            api_id: Some(Uuid::now_v7()),
            titel: "Test Document".to_string(),
            kurztitel: None,
            vorwort: Some("Test Vorwort".to_string()),
            volltext: "Test Volltext".to_string(),
            zusammenfassung: None,
            typ: openapi::models::Doktyp::Entwurf,
            link: "http://example.com/doc".to_string(),
            hash: "testhash".to_string(),
            zp_modifiziert: Utc::now(),
            drucksnr: None,
            zp_referenz: Utc::now(),
            zp_erstellt: Some(Utc::now()),
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
            dokumente: Some(vec![StationDokumenteInner::Dokument(Box::new(test_doc))]),
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
            touched_by: None,
            termin: Utc::now(),
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

    pub(crate) fn create_test_vorgang() -> models::Vorgang {
        use chrono::Utc;
        use openapi::models::{
            Autor, Doktyp, Dokument, Parlament, Station, StationDokumenteInner, Stationstyp,
            VgIdent, VgIdentTyp, Vorgang, Vorgangstyp,
        };
        use uuid::Uuid;

        // Create a test document
        let test_doc = Dokument {
            api_id: Some(Uuid::now_v7()),
            titel: "Test Document".to_string(),
            touched_by: None,
            kurztitel: None,
            vorwort: Some("Test Vorwort".to_string()),
            volltext: "Test Volltext".to_string(),
            zusammenfassung: None,
            typ: Doktyp::Entwurf,
            link: "http://example.com/doc".to_string(),
            hash: "testhash".to_string(),
            zp_modifiziert: Utc::now(),
            drucksnr: None,
            zp_referenz: Utc::now(),
            zp_erstellt: Some(Utc::now()),
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
            dokumente: vec![StationDokumenteInner::Dokument(Box::new(test_doc))],
            zp_start: Utc::now(),
            api_id: Some(Uuid::now_v7()),
            touched_by: None,
            titel: Some("Test Station".to_string()),
            gremium_federf: None,
            link: Some("http://example.com".to_string()),
            trojanergefahr: None,
            zp_modifiziert: Some(Utc::now()),
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
            lobbyregister: None,
            touched_by: None,
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
