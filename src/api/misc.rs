use std::str::FromStr;

use crate::{LTZFError, LTZFServer, Result};
use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::CookieJar;
use axum_extra::extract::Host;
use openapi::apis::data_administration_miscellaneous::*;
use openapi::apis::miscellaneous_unauthorisiert::*;
use openapi::models;

use super::PaginationResponsePart;

#[async_trait]
impl MiscellaneousUnauthorisiert<LTZFError> for LTZFServer {
    async fn autoren_get(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        query_params: &models::AutorenGetQueryParams,
    ) -> Result<AutorenGetResponse> {
        let mut tx = self.sqlx_db.begin().await?;
        let full_authors = sqlx::query!(
            "SELECT COUNT(1) as cnt FROM autor WHERE
            person = COALESCE($1, person) AND
            organisation = COALESCE($2, organisation) AND
            fachgebiet = COALESCE($3, fachgebiet)
            ",
            query_params.inipsn,
            query_params.iniorg,
            query_params.inifch
        )
        .map(|r| r.cnt)
        .fetch_one(&mut *tx)
        .await?
        .unwrap() as i32;

        let prp =
            PaginationResponsePart::new(full_authors, query_params.page, query_params.per_page);
        let output = sqlx::query!(
            "SELECT * FROM autor WHERE
            person = COALESCE($1, person) AND
            organisation = COALESCE($2, organisation) AND
            fachgebiet = COALESCE($3, fachgebiet)
            LIMIT $4 OFFSET $5
            ",
            query_params.inipsn,
            query_params.iniorg,
            query_params.inifch,
            prp.limit(),
            prp.offset()
        )
        .map(|r| models::Autor {
            fachgebiet: r.fachgebiet,
            person: r.person,
            lobbyregister: r.lobbyregister,
            organisation: r.organisation,
        })
        .fetch_all(&mut *tx)
        .await?;

        tx.commit().await?;
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
        let mut result = sqlx::query!(
            "SELECT g.id FROM gremium g
        INNER JOIN parlament p ON p.id = g.parl 
        WHERE p.value = COALESCE($1, p.value) AND
        g.wp = COALESCE($2, g.wp) AND
        ($3::text IS NULL OR g.name LIKE CONCAT('%',$3,'%'))
        ",
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
        // welche enums gibts?
        // - vorgangstyp
        // - vg-id-typ
        // - schlagwort
        // - dokumententyp
        // - stationstyp
        let contains = query_params
            .contains
            .as_ref()
            .map(|x| x.to_lowercase())
            .unwrap_or("".to_string());
        let mut tx = self.sqlx_db.begin().await?;
        let mut filtered_ids = match path_params.name.as_str() {
            "vorgangstyp" => {
                sqlx::query!(
                    "SELECT v.id FROM vorgangstyp v WHERE v.value LIKE CONCAT('%',$1::text,'%')",
                    contains
                )
                .map(|r| r.id)
                .fetch_all(&mut *tx)
                .await?
            }
            "vg-id-typ" => {
                sqlx::query!(
                    "SELECT v.id FROM vg_ident_typ v WHERE v.value LIKE CONCAT('%', $1::text, '%')",
                    contains
                )
                .map(|r| r.id)
                .fetch_all(&mut *tx)
                .await?
            }
            "schlagwort" => {
                sqlx::query!(
                    "SELECT v.id FROM schlagwort v WHERE v.value LIKE CONCAT('%', $1::text, '%')",
                    contains
                )
                .map(|r| r.id)
                .fetch_all(&mut *tx)
                .await?
            }
            "dokumententyp" => sqlx::query!(
                "SELECT v.id FROM dokumententyp v WHERE v.value LIKE CONCAT('%', $1::text, '%')",
                contains
            )
            .map(|r| r.id)
            .fetch_all(&mut *tx)
            .await?,
            "stationstyp" => {
                sqlx::query!(
                    "SELECT v.id FROM stationstyp v WHERE v.value LIKE CONCAT('%', $1::text, '%')",
                    contains
                )
                .map(|r| r.id)
                .fetch_all(&mut *tx)
                .await?
            }
            name => {
                tracing::warn!("enum_get called with unknown enumeration `{name}`");
                return Ok(EnumGetResponse::Status404_NotFound {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None,
                });
            }
        };
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
        let values = match path_params.name.as_str() {
            "vorgangstyp" => {
                sqlx::query!(
                    "SELECT v.value FROM vorgangstyp v WHERE v.id = ANY($1::int4[])",
                    &select_few[..]
                )
                .map(|r| r.value)
                .fetch_all(&mut *tx)
                .await?
            }
            "vg-id-typ" => {
                sqlx::query!(
                    "SELECT v.value FROM vg_ident_typ v WHERE v.id = ANY($1::int4[])",
                    &select_few[..]
                )
                .map(|r| r.value)
                .fetch_all(&mut *tx)
                .await?
            }
            "schlagwort" => {
                sqlx::query!(
                    "SELECT v.value FROM schlagwort v WHERE v.id = ANY($1::int4[])",
                    &select_few[..]
                )
                .map(|r| r.value)
                .fetch_all(&mut *tx)
                .await?
            }
            "dokumententyp" => {
                sqlx::query!(
                    "SELECT v.value FROM dokumententyp v WHERE v.id = ANY($1::int4[])",
                    &select_few[..]
                )
                .map(|r| r.value)
                .fetch_all(&mut *tx)
                .await?
            }
            "stationstyp" => {
                sqlx::query!(
                    "SELECT v.value FROM stationstyp v WHERE v.id = ANY($1::int4[])",
                    &select_few[..]
                )
                .map(|r| r.value)
                .fetch_all(&mut *tx)
                .await?
            }
            _ => unreachable!(
                "This portion of code cannot be reached since the condition is now checked for the second time"
            ),
        };

        return Ok(EnumGetResponse::Status200_Success {
            body: values,
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
            x_total_count: Some(prp.x_total_count),
            x_total_pages: Some(prp.x_total_pages),
            x_page: Some(prp.x_page),
            x_per_page: Some(prp.x_per_page),
            link: Some(prp.generate_link_header(&format!("/api/v1/enum/{}", path_params.name))),
        });
    }

    /// DokumentGetById - GET /api/v1/dokument/{api_id}
    async fn dokument_get_by_id(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
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
            let dok = crate::db::retrieve::dokument_by_id(did, &mut tx).await?;
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
                EnumGetResponse, GremienGetResponse, MiscellaneousUnauthorisiert,
            },
        },
        models,
    };

    use crate::utils::test::TestSetup;
    use crate::{api::auth::APIScope, utils::test::generate};

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
            .await;
        match result {
            Ok(GremienGetResponse::Status204_NoContent { .. }) => {}
            _ => {
                assert!(false, "Expected to find no entries")
            }
        }
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
        match rsp {
            openapi::apis::data_administration_vorgang::VorgangIdPutResponse::Status201_Created { .. } => {},
            xxx => assert!(false, "Expected succes, got {:?}", xxx)
        }

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
        match result {
            Ok(GremienGetResponse::Status200_Success { body, .. }) => {
                assert!(body.contains(&generate::default_gremium()));
                assert_eq!(body.len(), 1);
            }
            _ => {
                assert!(false, "Expected to find no entries")
            }
        }

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
        match result {
            Ok(GremienGetResponse::Status200_Success { body, .. }) => {
                assert!(body.contains(&generate::default_gremium()));
                assert_eq!(body.len(), 1);
            }
            got => {
                assert!(false, "Expected to find Some entries, got {:?}", got)
            }
        }

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
        match result {
            Ok(GremienGetResponse::Status200_Success { body, .. }) => {
                assert!(body.contains(&generate::default_gremium()));
                assert_eq!(body.len(), 1);
            }
            _ => {
                assert!(false, "Expected to find no entries")
            }
        }

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
        match result {
            Ok(GremienGetResponse::Status200_Success { body, .. }) => {
                assert!(body.contains(&generate::default_gremium()));
                assert_eq!(body.len(), 1);
            }
            _ => {
                assert!(false, "Expected to find no entries")
            }
        }

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
        match result {
            Ok(GremienGetResponse::Status204_NoContent { .. }) => {}
            _ => {
                assert!(false, "Expected to find no entries")
            }
        }
        scenario.teardown().await;
    }
    #[tokio::test]
    async fn test_enum_get_nocontent() {
        let scenario = TestSetup::new("test_enum_get_nocontent").await;
        let test_parameters = vec![
            "vorgangstyp",
            "vg-id-typ",
            "dokumententyp",
            "schlagwort",
            "stationstyp",
        ];
        for tp in test_parameters {
            let result = scenario
                .server
                .enum_get(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::EnumGetPathParams {
                        name: tp.to_string(),
                    },
                    &models::EnumGetQueryParams {
                        contains: Some("apfelsaftcocktail".to_string()), // hoffentlich kommt niemand auf die depperte idee apfelsaftcocktail-Gesetzgebung zu machen
                        page: None,
                        per_page: None,
                    },
                )
                .await;
            match result {
                Ok(EnumGetResponse::Status204_NoContent { .. }) => {}
                res => {
                    assert!(false, "Expected to find no entries, got {:?} instead", res)
                }
            }
        }
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn test_enum_get_notfound() {
        let scenario = TestSetup::new("test_enum_get_notfound").await;
        let result = scenario
            .server
            .enum_get(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &models::EnumGetPathParams {
                    name: "komplettandererwert".to_string(),
                },
                &models::EnumGetQueryParams {
                    contains: Some("apfelsaftcocktail".to_string()), // hoffentlich kommt niemand auf die depperte idee apfelsaftcocktail-Gesetzgebung zu machen
                    page: None,
                    per_page: None,
                },
            )
            .await;
        match result {
            Ok(EnumGetResponse::Status404_NotFound { .. }) => {}
            res => {
                assert!(false, "Expected to get \"notFound\", got {:?} instead", res)
            }
        }
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn test_enum_get_success() {
        let scenario = TestSetup::new("test_enum_get_success").await;
        let test_cases_one = vec![
            ("vorgangstyp", "einspruch"),
            ("vg-id-typ", "initdrucks"),
            ("dokumententyp", "entwurf"),
            ("stationstyp", "blt"),
            ("schlagwort", "schuppe"),
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
        match rsp {
            openapi::apis::data_administration_vorgang::VorgangIdPutResponse::Status201_Created { .. } => {},
            xxx => assert!(false, "Expected succes, got {:?}", xxx)
        }
        for (name, mtch) in test_cases_one {
            let result = scenario
                .server
                .enum_get(
                    &Method::GET,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &models::EnumGetPathParams {
                        name: name.to_string(),
                    },
                    &models::EnumGetQueryParams {
                        contains: Some(mtch.to_string()),
                        page: None,
                        per_page: None,
                    },
                )
                .await;
            match result {
                Ok(EnumGetResponse::Status200_Success { body, .. }) => {
                    assert!(!body.is_empty(), "with test case `{}` / `{}`", name, mtch);
                }
                res => {
                    assert!(
                        false,
                        "Expected to get success, got {:?} instead with test case `{}` / `{}`",
                        res, name, mtch
                    )
                }
            }
        }
        scenario.teardown().await;
    }
}

#[async_trait]
impl DataAdministrationMiscellaneous<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;
    /// AutorenDeleteByParam - DELETE /api/v1/autoren
    async fn autoren_delete_by_param(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        query_params: &models::AutorenDeleteByParamQueryParams,
    ) -> Result<AutorenDeleteByParamResponse> {
        todo!()
    }

    /// AutorenPut - PUT /api/v1/autoren
    async fn autoren_put(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        body: &models::AutorenPutRequest,
    ) -> Result<AutorenPutResponse> {
        todo!()
    }

    /// GremienDeleteByParam - DELETE /api/v1/gremien
    async fn gremien_delete_by_param(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        query_params: &models::GremienDeleteByParamQueryParams,
    ) -> Result<GremienDeleteByParamResponse> {
        todo!()
    }

    /// GremienPut - PUT /api/v1/gremien
    async fn gremien_put(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        body: &models::GremienPutRequest,
    ) -> Result<GremienPutResponse> {
        todo!()
    }

    /// EnumDelete - DELETE /api/v1/enumeration/{name}/{item}
    async fn enum_delete(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::EnumDeletePathParams,
    ) -> Result<EnumDeleteResponse> {
        todo!()
    }

    /// EnumPut - PUT /api/v1/enumeration/{name}
    async fn enum_put(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::EnumPutPathParams,
        body: &models::EnumPutRequest,
    ) -> Result<EnumPutResponse> {
        todo!()
    }

    /// DokumentDeleteId - DELETE /api/v1/dokument/{api_id}
    async fn dokument_delete_id(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::DokumentDeleteIdPathParams,
    ) -> Result<DokumentDeleteIdResponse> {
        if claims.0 != super::auth::APIScope::Admin && claims.0 != super::auth::APIScope::KeyAdder {
            return Ok(DokumentDeleteIdResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let mut tx = self.sqlx_db.begin().await?;
        sqlx::query!("DELETE FROM dokument WHERE api_id = $1", path_params.api_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(DokumentDeleteIdResponse::Status204_NoContent {
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
        });
    }

    /// DokumentPutId - PUT /api/v1/dokument/{api_id}
    async fn dokument_put_id(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::DokumentPutIdPathParams,
        body: &models::Dokument,
    ) -> Result<DokumentPutIdResponse> {
        if claims.0 != super::auth::APIScope::Admin && claims.0 != super::auth::APIScope::KeyAdder {
            return Ok(DokumentPutIdResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let mut tx = self.sqlx_db.begin().await?;
        let did = sqlx::query!(
            "SELECT id FROM dokument WHERE api_id = $1",
            path_params.api_id
        )
        .map(|r| r.id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(did) = did {
            let dok = crate::db::retrieve::dokument_by_id(did, &mut tx).await?;
            if super::compare::compare_dokument(&dok, body) {
                return Ok(DokumentPutIdResponse::Status304_NotModified {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None,
                });
            }
            sqlx::query!("DELETE FROM dokument WHERE api_id = $1", path_params.api_id)
                .execute(&mut *tx)
                .await?;
        }
        let _ = crate::db::insert::insert_dokument(body.clone(), &mut tx, self).await?;

        tx.commit().await?;
        return Ok(DokumentPutIdResponse::Status201_Created {
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
        });
    }
}
