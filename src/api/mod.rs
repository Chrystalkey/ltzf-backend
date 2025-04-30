use std::sync::Arc;

use async_trait::async_trait;
use axum_extra::extract::Host;

use crate::Configuration;
use crate::Result;
use crate::error::LTZFError;
use crate::utils::notify;
use openapi::apis::unauthorisiert::*;

pub(crate) mod auth;
pub(crate) mod compare;
pub(crate) mod misc;
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
pub struct PaginationResponsePart {
    pub x_total_count: Option<i32>,
    pub x_total_pages: Option<i32>,
    pub x_page: Option<i32>,
    pub x_per_page: Option<i32>,
    pub link: Option<String>,
}
impl PaginationResponsePart {
    const DEFAULT_PER_PAGE: i32 = 32;
    pub fn new(
        x_total_count: Option<i32>,
        x_page: Option<i32>,
        x_per_page: Option<i32>,
        link_first_part: &str,
    ) -> Self {
        let mut link_string = String::new();
        let x_total_pages = x_total_count
            .map(|x| (x as f32 / 32.).ceil() as i32)
            .unwrap_or(0);
        let x_per_page = x_per_page.unwrap_or(Self::DEFAULT_PER_PAGE);
        let x_page = x_page.unwrap_or(0);

        if x_page != x_total_pages {
            link_string = format!(
                "<\"{}?page={}&per_page={}\">; rel=\"next\", ",
                link_first_part,
                x_page + 1,
                x_per_page
            );
        }
        if x_page != 0 {
            link_string = format!(
                "{}<\"{}?page={}&per_page={}\">; rel=\"previous\", ",
                link_string,
                link_first_part,
                x_page - 1,
                x_per_page
            );
        }
        link_string = format!(
            "{}<\"{}?page={}&per_page={}\">; rel=\"first\", ",
            link_string, link_first_part, 0, x_per_page
        );
        link_string = format!(
            "{}<\"{}?page={}&per_page={}\">; rel=\"last\"",
            link_string, link_first_part, x_total_pages, x_per_page
        );
        Self {
            x_total_count: x_total_count,
            x_total_pages: Some(x_total_pages),
            x_page: Some(x_page),
            x_per_page: Some(x_per_page),
            link: Some(link_string),
        }
    }
    pub fn limit(&self) -> i64 {
        self.x_per_page.unwrap_or(Self::DEFAULT_PER_PAGE) as i64
    }
    pub fn offset(&self) -> i64 {
        (self.x_page.unwrap_or(0) * self.x_per_page.unwrap_or(Self::DEFAULT_PER_PAGE)) as i64
    }
}
#[cfg(test)]
pub(crate) mod endpoint_test {
    use super::*;
    use crate::{LTZFServer, Result};
    use openapi::models;
    use sha256::digest;
    const MASTER_URL: &str = "postgres://ltzf-user:ltzf-pass@localhost:5432/ltzf";

    pub(crate) async fn setup_server(dbname: &str) -> Result<LTZFServer> {
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

    pub(crate) async fn cleanup_server(dbname: &str) -> Result<()> {
        let create_pool = sqlx::PgPool::connect(MASTER_URL).await.unwrap();
        sqlx::query(&format!("DROP DATABASE {} WITH (FORCE);", dbname))
            .execute(&create_pool)
            .await?;
        Ok(())
    }

    // Session (Sitzung) tests
    pub(crate) fn create_test_session() -> models::Sitzung {
        use chrono::{DateTime, Utc};
        use openapi::models::{Autor, DokRef, Dokument, Gremium, Parlament, Sitzung, Top};
        use uuid::Uuid;

        // Create a test document
        let test_doc = Dokument {
            touched_by: None,
            dc_type: std::default::Default::default(),
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
            touched_by: None,
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

    pub(crate) fn create_test_vorgang() -> models::Vorgang {
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
            dc_type: std::default::Default::default(),
            touched_by: None,
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
            touched_by: None,
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
