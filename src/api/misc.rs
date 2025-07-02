use std::str::FromStr;

use crate::api::auth::APIScope;
use crate::utils::as_option;
use crate::{LTZFError, LTZFServer, Result};
use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::CookieJar;
use axum_extra::extract::Host;
use openapi::apis::miscellaneous_unauthorisiert::*;
use openapi::models;
use sqlx::Row;

use super::PaginationResponsePart;

#[async_trait]
impl MiscellaneousUnauthorisiert<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;
    async fn autoren_get(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        query_params: &models::AutorenGetQueryParams,
    ) -> Result<AutorenGetResponse> {
        let mut tx = self.sqlx_db.begin().await?;
        tracing::info!("Autoren Get with Query Params {:?}", query_params);
        let result = sqlx::query!(
            "SELECT a.id FROM autor a WHERE
            ($1::text IS NULL AND person IS NULL OR person LIKE CONCAT('%',$1,'%')) AND
            organisation LIKE CONCAT('%',$2::text,'%') AND
            ($3::text IS NULL AND fachgebiet IS NULL OR fachgebiet LIKE CONCAT('%', $3, '%'))
            ",
            query_params.person,
            query_params.org,
            query_params.fach,
        )
        .map(|r| r.id)
        .fetch_all(&mut *tx)
        .await?;

        let prp = PaginationResponsePart::new(
            result.len() as i32,
            query_params.page,
            query_params.per_page,
        );
        let result = &result[prp.start()..prp.end()];
        let output = sqlx::query!(
            "SELECT * FROM autor WHERE id = ANY($1::int4[])",
            &result[..]
        )
        .map(|r| models::Autor {
            fachgebiet: r.fachgebiet,
            lobbyregister: r.lobbyregister,
            organisation: r.organisation,
            person: r.person,
        })
        .fetch_all(&mut *tx)
        .await?;

        tx.commit().await?;

        if output.is_empty() {
            return Ok(AutorenGetResponse::Status204_NoContent {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        return Ok(AutorenGetResponse::Status200_Success {
            body: output,
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
            x_total_count: Some(prp.x_total_count),
            x_total_pages: Some(prp.x_total_pages),
            x_page: Some(prp.x_page),
            x_per_page: Some(prp.x_per_page),
            link: Some(prp.generate_link_header("/api/v1/autoren")),
        });
    }

    async fn gremien_get(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        query_params: &models::GremienGetQueryParams,
    ) -> Result<GremienGetResponse> {
        let mut tx = self.sqlx_db.begin().await?;
        tracing::info!("Gremien Get with Query Params {:?}", query_params);
        let mut result = sqlx::query!(
            "SELECT g.id FROM gremium g
        INNER JOIN parlament p ON p.id = g.parl 
        WHERE p.value = COALESCE($1, p.value) AND
        g.wp = COALESCE($2, g.wp) AND
        ($3::text IS NULL OR g.name LIKE CONCAT('%',$3,'%'))",
            query_params.p.map(|x| x.to_string()),
            query_params.wp,
            query_params.gr
        )
        .map(|r| r.id)
        .fetch_all(&mut *tx)
        .await?;
        if result.is_empty() {
            return Ok(GremienGetResponse::Status204_NoContent {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let prp = PaginationResponsePart::new(
            result.len() as i32,
            query_params.page,
            query_params.per_page,
        );
        let selected_ids: Vec<i32> = result.drain(prp.start()..prp.end()).collect();
        let result = sqlx::query!(
            "SELECT g.link, g.name, g.wp, p.value as parl FROM gremium g
        INNER JOIN parlament p ON p.id = g.parl
        WHERE g.id = ANY($1::int4[])",
            &selected_ids[..]
        )
        .map(|r| models::Gremium {
            link: r.link,
            name: r.name,
            parlament: models::Parlament::from_str(&r.parl).unwrap(),
            wahlperiode: r.wp as u32,
        })
        .fetch_all(&mut *tx)
        .await?;
        Ok(GremienGetResponse::Status200_Success {
            body: result,
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
            x_total_count: Some(prp.x_total_count),
            x_total_pages: Some(prp.x_total_pages),
            x_page: Some(prp.x_page),
            x_per_page: Some(prp.x_per_page),
            link: Some(prp.generate_link_header("/api/v1/gremien")),
        })
    }

    /// EnumGet - GET /api/v1/enumeration/{name}
    async fn enum_get(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        path_params: &models::EnumGetPathParams,
        query_params: &models::EnumGetQueryParams,
    ) -> Result<EnumGetResponse> {
        let contains = query_params
            .contains
            .as_ref()
            .map(|x| {
                if path_params.name == models::EnumerationNames::Parlamente {
                    x.to_uppercase()
                } else {
                    x.to_lowercase()
                }
            })
            .unwrap_or("".to_string());
        let mut tx = self.sqlx_db.begin().await?;
        let enum_tables = std::collections::BTreeMap::from_iter(
            vec![
                (models::EnumerationNames::Schlagworte, "schlagwort"),
                (models::EnumerationNames::Stationstypen, "stationstyp"),
                (models::EnumerationNames::Parlamente, "parlament"),
                (models::EnumerationNames::Vorgangstypen, "vorgangstyp"),
                (models::EnumerationNames::Dokumententypen, "dokumententyp"),
                (models::EnumerationNames::Vgidtypen, "vg_ident_typ"),
            ]
            .drain(..),
        );
        let mut filtered_ids = sqlx::query(&format!(
            "SELECT v.id FROM {} v WHERE v.value LIKE CONCAT('%',$1::text,'%')",
            enum_tables[&path_params.name]
        ))
        .bind::<_>(contains)
        .map(|r| r.get(0))
        .fetch_all(&mut *tx)
        .await?;

        if filtered_ids.is_empty() {
            return Ok(EnumGetResponse::Status204_NoContent {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }

        let prp = PaginationResponsePart::new(
            filtered_ids.len() as i32,
            query_params.page,
            query_params.per_page,
        );
        let select_few: Vec<i32> = filtered_ids.drain(prp.start()..prp.end()).collect();
        let values: Vec<String> = sqlx::query(&format!(
            "SELECT v.value FROM {} v WHERE v.id = ANY($1::int4[])",
            enum_tables[&path_params.name]
        ))
        .bind::<_>(select_few)
        .map(|r| r.get(0))
        .fetch_all(&mut *tx)
        .await?;

        return Ok(EnumGetResponse::Status200_Success {
            body: values,
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
            x_total_count: Some(prp.x_total_count),
            x_total_pages: Some(prp.x_total_pages),
            x_page: Some(prp.x_page),
            x_per_page: Some(prp.x_per_page),
            link: Some(
                prp.generate_link_header(&format!("/api/v1/enumeration/{}", path_params.name)),
            ),
        });
    }

    /// DokumentGetById - GET /api/v1/dokument/{api_id}
    async fn dokument_get_by_id(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::DokumentGetByIdPathParams,
    ) -> Result<DokumentGetByIdResponse> {
        let mut tx = self.sqlx_db.begin().await?;
        let did = sqlx::query!(
            "SELECT id FROM dokument WHERE api_id = $1",
            path_params.api_id
        )
        .map(|r| r.id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(did) = did {
            let mut dok = crate::db::retrieve::dokument_by_id(did, &mut tx).await?;
            if claims.0 == APIScope::KeyAdder || claims.0 == APIScope::Admin {
                dok.touched_by = as_option(
                    sqlx::query!(
                        "SELECT * FROM scraper_touched_dokument sts
                    INNER JOIN api_keys ON api_keys.id = sts.collector_key
                    WHERE dok_id = $1",
                        did
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
            return Ok(DokumentGetByIdResponse::Status200_Success {
                body: dok,
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        return Ok(DokumentGetByIdResponse::Status404_NotFound {
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
        });
    }
}
#[cfg(test)]
mod test_unauthorisiert {
    use axum::http::Method;
    use axum_extra::extract::{CookieJar, Host};
    use openapi::{
        apis::{
            data_administration_vorgang::DataAdministrationVorgang,
            miscellaneous_unauthorisiert::{
                AutorenGetResponse, EnumGetResponse, GremienGetResponse,
                MiscellaneousUnauthorisiert,
            },
        },
        models,
    };

    use crate::utils::test::TestSetup;
    use crate::{api::auth::APIScope, utils::test::generate};
    #[tokio::test]
    async fn test_autor_get_nocontent() {
        let scenario = TestSetup::new("autor_get_nocontent").await;
        let r = scenario
            .server
            .autoren_get(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &models::AutorenGetQueryParams {
                    fach: None,
                    org: None,
                    person: None,
                    page: None,
                    per_page: None,
                },
            )
            .await
            .unwrap();
        assert!(matches!(r, AutorenGetResponse::Status204_NoContent { .. }));
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn test_autor_get_success() {
        let scenario = TestSetup::new("autor_get_success").await;

        let vorgang = generate::default_vorgang();
        let rsp = scenario
            .server
            .vorgang_id_put(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::KeyAdder, 1),
                &models::VorgangIdPutPathParams {
                    vorgang_id: vorgang.api_id,
                },
                &vorgang,
            )
            .await
            .unwrap();
        assert!(matches!(rsp, openapi::apis::data_administration_vorgang::VorgangIdPutResponse::Status201_Created { .. }), "Expected succes, got {rsp:?}");

        let r = scenario
            .server
            .autoren_get(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &models::AutorenGetQueryParams {
                    fach: None,
                    org: None,
                    person: None,
                    page: None,
                    per_page: None,
                },
            )
            .await
            .unwrap();
        assert!(
            match &r {
                AutorenGetResponse::Status200_Success { body, .. } => {
                    assert!(!body.is_empty(), "Body is empty, expected some object");
                    true
                }
                _ => false,
            },
            "Expected Successful response, got {r:?}"
        );
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn test_gremien_get_nocontent() {
        let scenario = TestSetup::new("test_gremien_get_nocontent").await;
        let result = scenario
            .server
            .gremien_get(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &models::GremienGetQueryParams {
                    gr: None,
                    p: None,
                    page: None,
                    per_page: None,
                    wp: None,
                },
            )
            .await
            .unwrap();
        assert!(
            matches!(result, GremienGetResponse::Status204_NoContent { .. }),
            "Expected to find no entries"
        );
        scenario.teardown().await;
    }
    #[tokio::test]
    async fn test_gremium_get_success() {
        let scenario = TestSetup::new("test_gremium_get_success").await;
        let vorgang = generate::default_vorgang();
        let rsp = scenario
            .server
            .vorgang_id_put(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::KeyAdder, 1),
                &models::VorgangIdPutPathParams {
                    vorgang_id: vorgang.api_id,
                },
                &vorgang,
            )
            .await
            .unwrap();
        assert!(matches!(rsp, openapi::apis::data_administration_vorgang::VorgangIdPutResponse::Status201_Created { .. }), "Expected succes, got {rsp:?}");

        let result = scenario
            .server
            .gremien_get(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &models::GremienGetQueryParams {
                    gr: None,
                    p: None,
                    page: None,
                    per_page: None,
                    wp: None,
                },
            )
            .await;

        assert!(
            matches!(&result, Ok(GremienGetResponse::Status200_Success { body, .. }) if body.len() == 1 && body.contains(&generate::default_gremium())),
            "Expected to find exactly one entry"
        );

        let result = scenario
            .server
            .gremien_get(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &models::GremienGetQueryParams {
                    gr: Some("Inneres".to_string()),
                    p: None,
                    page: None,
                    per_page: None,
                    wp: None,
                },
            )
            .await;
        assert!(
            matches!(&result, Ok(GremienGetResponse::Status200_Success { body, .. }) if body.len() == 1 && body.contains(&generate::default_gremium())),
            "Expected to find Some entries, got {result:?}"
        );

        let result = scenario
            .server
            .gremien_get(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &models::GremienGetQueryParams {
                    gr: None,
                    p: Some(models::Parlament::Bb),
                    page: None,
                    per_page: None,
                    wp: None,
                },
            )
            .await;
        assert!(
            matches!(&result, Ok(GremienGetResponse::Status200_Success { body, .. }) if body.len() == 1 && body.contains(&generate::default_gremium())),
            "Expected to find no entries"
        );

        let result = scenario
            .server
            .gremien_get(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &models::GremienGetQueryParams {
                    gr: None,
                    p: None,
                    page: None,
                    per_page: None,
                    wp: Some(20),
                },
            )
            .await;
        assert!(
            matches!(&result, Ok(GremienGetResponse::Status200_Success { body, .. }) if body.len() == 1 && body.contains(&generate::default_gremium())),
            "Expected to find no entries"
        );

        let result = scenario
            .server
            .gremien_get(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &models::GremienGetQueryParams {
                    gr: None,
                    p: Some(models::Parlament::Be),
                    page: None,
                    per_page: None,
                    wp: None,
                },
            )
            .await;
        assert!(
            matches!(&result, Ok(GremienGetResponse::Status204_NoContent { .. })),
            "Expected to find no entries"
        );
        scenario.teardown().await;
    }
    #[tokio::test]
    async fn test_enum_get_nocontent() {
        let scenario = TestSetup::new("test_enum_get_nocontent").await;
        let test_parameters = vec![
            models::EnumerationNames::Parlamente,
            models::EnumerationNames::Dokumententypen,
            models::EnumerationNames::Vgidtypen,
            models::EnumerationNames::Vorgangstypen,
            models::EnumerationNames::Stationstypen,
            models::EnumerationNames::Schlagworte,
        ];
        for tp in test_parameters {
            let result = scenario
                .server
                .enum_get(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::EnumGetPathParams { name: tp },
                    &models::EnumGetQueryParams {
                        contains: Some("apfelsaftcocktail".to_string()), // hoffentlich kommt niemand auf die depperte idee apfelsaftcocktail-Gesetzgebung zu machen
                        page: None,
                        per_page: None,
                    },
                )
                .await;
            assert!(
                matches!(&result, Ok(EnumGetResponse::Status204_NoContent { .. })),
                "Expected to find no entries, got {result:?} instead"
            );
        }
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn test_enum_get_success() {
        let scenario = TestSetup::new("test_enum_get_success").await;
        let test_cases_one = vec![
            (models::EnumerationNames::Vorgangstypen, "einspruch"),
            (models::EnumerationNames::Vgidtypen, "initdrucks"),
            (models::EnumerationNames::Dokumententypen, "entwurf"),
            (models::EnumerationNames::Stationstypen, "blt"),
            (models::EnumerationNames::Schlagworte, "schuppe"),
            (models::EnumerationNames::Parlamente, "B"),
        ];
        let vorgang = generate::default_vorgang();
        let rsp = scenario
            .server
            .vorgang_id_put(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::KeyAdder, 1),
                &models::VorgangIdPutPathParams {
                    vorgang_id: vorgang.api_id,
                },
                &vorgang,
            )
            .await
            .unwrap();
        assert!(matches!(&rsp, openapi::apis::data_administration_vorgang::VorgangIdPutResponse::Status201_Created { .. }), "Expected succes, got {rsp:?}");

        for (name, mtch) in test_cases_one {
            let result = scenario
                .server
                .enum_get(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::EnumGetPathParams { name },
                    &models::EnumGetQueryParams {
                        contains: Some(mtch.to_string()),
                        page: None,
                        per_page: None,
                    },
                )
                .await;
            assert!(
                matches!(&result, Ok(EnumGetResponse::Status200_Success { body, .. }) if !body.is_empty() ),
                "Expected to get success, got {result:?} instead with test case `{name}` / `{mtch}`"
            );
        }
        scenario.teardown().await;
    }
}
