use std::str::FromStr;

use crate::api::auth::APIScope;
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
        let author_count = sqlx::query!(
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
            PaginationResponsePart::new(author_count, query_params.page, query_params.per_page);
        let output = sqlx::query!(
            "SELECT * FROM autor WHERE
            ($1::text IS NULL OR person = COALESCE($1, person)) AND
            organisation = COALESCE($2, organisation) AND
            ($3::text IS NULL OR fachgebiet = COALESCE($3, fachgebiet))
            ORDER BY organisation ASC, person ASC, fachgebiet ASC
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
        if output.is_empty() {
            return Ok(AutorenGetResponse::Status204_NoContent {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
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
        let contains = query_params
            .contains
            .as_ref()
            .map(|x| x.to_lowercase())
            .unwrap_or("".to_string());
        let mut tx = self.sqlx_db.begin().await?;
        let mut filtered_ids = match path_params.name {
            models::EnumerationNames::Vorgangstypen => {
                sqlx::query!(
                    "SELECT v.id FROM vorgangstyp v WHERE v.value LIKE CONCAT('%',$1::text,'%')",
                    contains
                )
                .map(|r| r.id)
                .fetch_all(&mut *tx)
                .await?
            }
            models::EnumerationNames::Vgidtypen => {
                sqlx::query!(
                    "SELECT v.id FROM vg_ident_typ v WHERE v.value LIKE CONCAT('%', $1::text, '%')",
                    contains
                )
                .map(|r| r.id)
                .fetch_all(&mut *tx)
                .await?
            }
            models::EnumerationNames::Schlagworte => {
                sqlx::query!(
                    "SELECT v.id FROM schlagwort v WHERE v.value LIKE CONCAT('%', $1::text, '%')",
                    contains
                )
                .map(|r| r.id)
                .fetch_all(&mut *tx)
                .await?
            }
            models::EnumerationNames::Dokumententypen => sqlx::query!(
                "SELECT v.id FROM dokumententyp v WHERE v.value LIKE CONCAT('%', $1::text, '%')",
                contains
            )
            .map(|r| r.id)
            .fetch_all(&mut *tx)
            .await?,
            models::EnumerationNames::Stationstypen => {
                sqlx::query!(
                    "SELECT v.id FROM stationstyp v WHERE v.value LIKE CONCAT('%', $1::text, '%')",
                    contains
                )
                .map(|r| r.id)
                .fetch_all(&mut *tx)
                .await?
            }
            models::EnumerationNames::Parlamente => {
                let contains = contains.to_uppercase();
                sqlx::query!(
                    "SELECT v.id FROM parlament v WHERE v.value LIKE CONCAT('%', $1::text, '%')",
                    contains
                )
                .map(|r| r.id)
                .fetch_all(&mut *tx)
                .await?
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
        let values = match path_params.name {
            models::EnumerationNames::Vorgangstypen => {
                sqlx::query!(
                    "SELECT v.value FROM vorgangstyp v WHERE v.id = ANY($1::int4[])",
                    &select_few[..]
                )
                .map(|r| r.value)
                .fetch_all(&mut *tx)
                .await?
            }
            models::EnumerationNames::Vgidtypen => {
                sqlx::query!(
                    "SELECT v.value FROM vg_ident_typ v WHERE v.id = ANY($1::int4[])",
                    &select_few[..]
                )
                .map(|r| r.value)
                .fetch_all(&mut *tx)
                .await?
            }
            models::EnumerationNames::Schlagworte => {
                sqlx::query!(
                    "SELECT v.value FROM schlagwort v WHERE v.id = ANY($1::int4[])",
                    &select_few[..]
                )
                .map(|r| r.value)
                .fetch_all(&mut *tx)
                .await?
            }
            models::EnumerationNames::Dokumententypen => {
                sqlx::query!(
                    "SELECT v.value FROM dokumententyp v WHERE v.id = ANY($1::int4[])",
                    &select_few[..]
                )
                .map(|r| r.value)
                .fetch_all(&mut *tx)
                .await?
            }
            models::EnumerationNames::Stationstypen => {
                sqlx::query!(
                    "SELECT v.value FROM stationstyp v WHERE v.id = ANY($1::int4[])",
                    &select_few[..]
                )
                .map(|r| r.value)
                .fetch_all(&mut *tx)
                .await?
            }
            models::EnumerationNames::Parlamente => {
                sqlx::query!(
                    "SELECT v.value FROM parlament v WHERE v.id = ANY($1::int4[])",
                    &select_few[..]
                )
                .map(|r| r.value)
                .fetch_all(&mut *tx)
                .await?
            }
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
                    inifch: None,
                    iniorg: None,
                    inipsn: None,
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
        assert!(matches!(rsp, openapi::apis::data_administration_vorgang::VorgangIdPutResponse::Status201_Created { .. }), "Expected succes, got {:?}", rsp);

        let r = scenario
            .server
            .autoren_get(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &models::AutorenGetQueryParams {
                    inifch: None,
                    iniorg: None,
                    inipsn: None,
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
            "Expected Successful response, got {:?}",
            r
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
        assert!(matches!(rsp, openapi::apis::data_administration_vorgang::VorgangIdPutResponse::Status201_Created { .. }), "Expected succes, got {:?}", rsp);

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
            "Expected to find Some entries, got {:?}",
            result
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
                "Expected to find no entries, got {:?} instead",
                result
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
        assert!(matches!(&rsp, openapi::apis::data_administration_vorgang::VorgangIdPutResponse::Status201_Created { .. }), "Expected succes, got {:?}", rsp);

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
                "Expected to get success, got {:?} instead with test case `{}` / `{}`",
                result,
                name,
                mtch
            );
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
        if claims.0 != APIScope::KeyAdder && claims.0 != APIScope::Admin {
            return Ok(AutorenDeleteByParamResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let empty_qp = models::AutorenDeleteByParamQueryParams {
            inipsn: None,
            inifch: None,
            iniorg: None,
        };
        if *query_params == empty_qp {
            return Ok(AutorenDeleteByParamResponse::Status204_NoContent {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let mut tx = self.sqlx_db.begin().await?;
        sqlx::query!(
            "
        DELETE FROM autor a WHERE 
        (a.person IS NULL OR a.person = COALESCE($1, a.person)) AND
        a.organisation = COALESCE($2, a.organisation) AND
        (a.fachgebiet IS NULL OR a.fachgebiet = COALESCE($3, a.fachgebiet))
        ",
            query_params.inipsn,
            query_params.iniorg,
            query_params.inifch
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(AutorenDeleteByParamResponse::Status204_NoContent {
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
        });
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
        if claims.0 != APIScope::KeyAdder && claims.0 != APIScope::Admin {
            return Ok(GremienDeleteByParamResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let empty_qp = models::GremienDeleteByParamQueryParams {
            gr: None,
            p: None,
            wp: None,
        };
        if *query_params == empty_qp {
            return Ok(GremienDeleteByParamResponse::Status204_NoContent {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        let mut tx = self.sqlx_db.begin().await?;
        sqlx::query!(
            "
        DELETE FROM gremium g WHERE 
        g.name = COALESCE($1, g.name) AND
        g.wp = COALESCE($2, g.wp) AND
        g.parl = COALESCE((SELECT id FROM parlament p WHERE p.value = $3), g.parl)
        ",
            query_params.gr,
            query_params.wp,
            query_params.p.as_ref().map(|x| x.to_string())
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(GremienDeleteByParamResponse::Status204_NoContent {
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
        });
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
        if claims.0 != APIScope::KeyAdder && claims.0 != APIScope::Admin {
            return Ok(EnumDeleteResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        use models::EnumerationNames::*;
        let mut tx = self.sqlx_db.begin().await?;
        match path_params.name {
            Schlagworte => {
                sqlx::query!(
                    "DELETE FROM schlagwort s WHERE s.value = $1",
                    path_params.item
                )
                .execute(&mut *tx)
                .await?;
            }
            Stationstypen => {
                sqlx::query!(
                    "DELETE FROM stationstyp s WHERE s.value = $1",
                    path_params.item
                )
                .execute(&mut *tx)
                .await?;
            }
            Vorgangstypen => {
                sqlx::query!(
                    "DELETE FROM vorgangstyp s WHERE s.value = $1",
                    path_params.item
                )
                .execute(&mut *tx)
                .await?;
            }
            Parlamente => {
                sqlx::query!(
                    "DELETE FROM parlament s WHERE s.value = $1",
                    path_params.item
                )
                .execute(&mut *tx)
                .await?;
            }
            Vgidtypen => {
                sqlx::query!(
                    "DELETE FROM vg_ident_typ s WHERE s.value = $1",
                    path_params.item
                )
                .execute(&mut *tx)
                .await?;
            }
            Dokumententypen => {
                sqlx::query!(
                    "DELETE FROM dokumententyp s WHERE s.value = $1",
                    path_params.item
                )
                .execute(&mut *tx)
                .await?;
            }
        }
        Ok(EnumDeleteResponse::Status204_NoContent {
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
        })
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
        if claims.0 != APIScope::KeyAdder && claims.0 != APIScope::Admin {
            return Ok(AutorenPutResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        todo!("{:?}", body)
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
        if claims.0 != APIScope::KeyAdder && claims.0 != APIScope::Admin {
            return Ok(GremienPutResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        // check if replacing contain an index larger than the object list
        // if so: Bad Request
        if let Some(replc) = &body.replacing {
            for rpl in replc.iter() {
                if rpl.replaced_by as usize >= body.objects.len() {
                    return Ok(GremienPutResponse::Status400_BadRequest {
                        x_rate_limit_limit: None,
                        x_rate_limit_remaining: None,
                        x_rate_limit_reset: None,
                    });
                }
            }
        }
        let mut tx = self.sqlx_db.begin().await?;
        // check if all gremien are existent in the database
        // check if none of the replacing gremien are in the database
        // if both: NotModified

        let (mut names, mut pvalues, mut wps, mut links) = (vec![], vec![], vec![], vec![]);
        for gr in body.objects.iter() {
            names.push(gr.name.clone());
            pvalues.push(gr.parlament.to_string());
            wps.push(gr.wahlperiode as i32);
            links.push(gr.link.clone());
        }
        if gremien_all_exist(&mut tx, &body.objects).await? {
            // flatten the replacement objects and check for existence
            if let Some(repl) = &body.replacing {
                let flattened: Vec<models::Gremium> = repl
                    .iter()
                    .flat_map(|o| o.values.iter())
                    .cloned()
                    .collect();
                if gremien_all_exist(&mut tx, &flattened).await? {
                    return Ok(GremienPutResponse::Status304_NotModified {
                        x_rate_limit_limit: None,
                        x_rate_limit_remaining: None,
                        x_rate_limit_reset: None,
                    });
                }
            } else {
                return Ok(GremienPutResponse::Status304_NotModified {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None,
                });
            }
        }

        // insert all gremien, fetch their IDs
        let (mut names, mut pvalues, mut wps, mut links) = (vec![], vec![], vec![], vec![]);
        for gr in body.objects.iter() {
            names.push(gr.name.clone());
            pvalues.push(gr.parlament.to_string());
            wps.push(gr.wahlperiode as i32);
            links.push(gr.link.clone());
        }
        let new_ids = sqlx::query!("
        INSERT INTO gremium(name, parl, wp, link) 
        
        SELECT nm, p.id, wp, ln FROM UNNEST($1::text[], $2::text[], $3::int4[], $4::text[]) AS iv(nm, pname, wp, ln)
        INNER JOIN parlament p ON p.value = iv.pname

        ON CONFLICT ON CONSTRAINT unique_combo 
        DO UPDATE SET link = EXCLUDED.link

        RETURNING gremium.id
        ", &names[..], &pvalues[..], &wps[..], &links[..] as &[Option<String>])
        .map(|r| r.id)
        .fetch_all(&mut *tx).await?;

        if body.replacing.is_none() {
            tx.commit().await?;
            // if there is nothing to replace, we are done here
            // CAREFUL: HERE DANGLING GREMIUM ENTRIES ARE CREATED
            return Ok(GremienPutResponse::Status201_Created {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        // for each replacing gremium:
        // for each table referencing it: Update those tables with the new id
        // tables that reference a gremium:
        // - station(gr_id)
        // - sitzung(gr_id)
        let mut replacement_tuples = vec![];
        for entry in body.replacing.as_ref().unwrap().iter() {
            let (mut vnames, mut vwps, mut vpvals) = (vec![], vec![], vec![]);
            for value in entry.values.iter() {
                vnames.push(value.name.clone());
                vwps.push(value.wahlperiode as i32);
                vpvals.push(value.parlament.to_string());
            }
            let value_ids: Vec<_> = sqlx::query!(
                "SELECT $4::int4 as repl_with, g.id as origin FROM
                UNNEST($1::text[], $2::text[], $3::int4[]) as iv(nm, pv, wp)
                INNER JOIN parlament p ON p.value = iv.pv
                INNER JOIN gremium g ON 
                g.name=iv.nm AND g.parl = p.id AND g.wp=iv.wp",
                &vnames[..],
                &vpvals[..],
                &vwps[..],
                new_ids[entry.replaced_by as usize] as i32
            )
            .map(|r| (r.repl_with.unwrap(), r.origin))
            .fetch_all(&mut *tx)
            .await?;
            replacement_tuples.extend(value_ids);
        }
        let rep_new: Vec<_> = replacement_tuples.iter().map(|x| x.0).collect();
        let rep_old: Vec<_> = replacement_tuples.iter().map(|x| x.1).collect();

        sqlx::query!(
            "
        WITH lookup AS (SELECT * FROM UNNEST($1::int4[], $2::int4[]) AS la(new, old))
        UPDATE station 
        SET gr_id = (SELECT new FROM lookup WHERE old=gr_id)
        WHERE gr_id = ANY($2::int4[])",
            &rep_new[..],
            &rep_old[..]
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            "
        WITH lookup AS (SELECT * FROM UNNEST($1::int4[], $2::int4[]) AS la(new, old))
        UPDATE sitzung 
        SET gr_id = (SELECT new FROM lookup WHERE old=gr_id)
        WHERE gr_id = ANY($2::int4[])",
            &rep_new[..],
            &rep_old[..]
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            "DELETE FROM gremium g WHERE g.id = ANY($1::int4[])",
            &rep_old[..]
        )
        .execute(&mut *tx)
        .await?;

        // return 201Created
        tx.commit().await?;
        Ok(GremienPutResponse::Status201_Created {
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
        })
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
        if claims.0 != APIScope::KeyAdder && claims.0 != APIScope::Admin {
            return Ok(EnumPutResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        todo!("{:?} // {:?}", path_params, body);
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
async fn gremien_all_exist(
    tx: &mut sqlx::PgTransaction<'_>,
    gremien: &[models::Gremium],
) -> Result<bool> {
    let (mut names, mut pvalues, mut wps, mut links) = (vec![], vec![], vec![], vec![]);
    for gr in gremien.iter() {
        names.push(gr.name.clone());
        pvalues.push(gr.parlament.to_string());
        wps.push(gr.wahlperiode as i32);
        links.push(gr.link.clone());
    }

    let existing_obj_cnt = sqlx::query!("SELECT COUNT(1) as cnt FROM 
        UNNEST($1::text[], $2::text[], $3::int4[], $4::text[]) as input_vector(name, pvalue, wp, link)
        WHERE EXISTS (
            SELECT 1 FROM gremium g 
        INNER JOIN parlament p ON g.parl = p.id
        WHERE 
            g.name = input_vector.name AND
            p.value = input_vector.pvalue AND
            g.wp = input_vector.wp AND
            (g.link IS NULL AND input_vector.link IS NULL OR g.link = input_vector.link)
        )
        ", &names[..], &pvalues[..], &wps[..], &links[..] as &[Option<String>])
        .map(|r| r.cnt)
        .fetch_one(&mut **tx).await?.unwrap();

    Ok(existing_obj_cnt as usize == gremien.len())
}
#[cfg(test)]
mod test_authorisiert {
    use std::str::FromStr;

    use crate::api::auth::APIScope;
    use axum::http::Method;
    use axum_extra::extract::{CookieJar, Host};
    use openapi::apis::data_administration_miscellaneous::{
        AutorenDeleteByParamResponse, DataAdministrationMiscellaneous, EnumDeleteResponse,
        GremienDeleteByParamResponse, GremienPutResponse,
    };
    use openapi::apis::data_administration_vorgang::DataAdministrationVorgang;
    use openapi::apis::miscellaneous_unauthorisiert::{
        GremienGetResponse, MiscellaneousUnauthorisiert,
    };
    use openapi::models;

    use crate::LTZFServer;
    use crate::utils::test::{TestSetup, generate};

    async fn insert_default_vorgang(server: &LTZFServer) {
        let vorgang = generate::default_vorgang();
        let rsp = server
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
        assert!(matches!(&rsp, openapi::apis::data_administration_vorgang::VorgangIdPutResponse::Status201_Created { .. }), "Expected succes, got {:?}", rsp);
    }
    async fn fetch_all_authors(server: &LTZFServer) -> Vec<models::Autor> {
        let autoren = server
            .autoren_get(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &models::AutorenGetQueryParams {
                    page: None,
                    per_page: None,
                    inifch: None,
                    iniorg: None,
                    inipsn: None,
                },
            )
            .await
            .unwrap();
        match autoren {
            openapi::apis::miscellaneous_unauthorisiert::AutorenGetResponse::Status200_Success { body, ..} => body,
            _ => vec![]
        }
    }
    async fn fetch_all_gremien(server: &LTZFServer) -> Vec<models::Gremium> {
        let autoren = server
            .gremien_get(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &models::GremienGetQueryParams {
                    page: None,
                    per_page: None,
                    gr: None,
                    p: None,
                    wp: None,
                },
            )
            .await
            .unwrap();
        match autoren {
            GremienGetResponse::Status200_Success { body, .. } => body,
            _ => vec![],
        }
    }
    #[tokio::test]
    async fn test_autor_delete() {
        let scenario = TestSetup::new("test_autor_delete").await;
        let r = scenario
            .server
            .autoren_delete_by_param(
                &Method::DELETE,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::Collector, 1),
                &models::AutorenDeleteByParamQueryParams {
                    inifch: None,
                    iniorg: None,
                    inipsn: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            r,
            AutorenDeleteByParamResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            }
        );
        insert_default_vorgang(&scenario.server).await;
        let autoren = fetch_all_authors(&scenario.server).await;
        let r = scenario
            .server
            .autoren_delete_by_param(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::KeyAdder, 1),
                &models::AutorenDeleteByParamQueryParams {
                    inifch: None,
                    iniorg: None,
                    inipsn: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            r,
            AutorenDeleteByParamResponse::Status204_NoContent {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            }
        );
        assert_eq!(
            autoren,
            fetch_all_authors(&scenario.server).await,
            "Expected no deleted item due to no filter applied"
        );

        let r = scenario
            .server
            .autoren_delete_by_param(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::KeyAdder, 1),
                &models::AutorenDeleteByParamQueryParams {
                    inifch: None,
                    iniorg: Some("Mysterium der Ministerien".to_string()),
                    inipsn: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            r,
            AutorenDeleteByParamResponse::Status204_NoContent {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            }
        );
        let autoren_now = fetch_all_authors(&scenario.server).await;
        assert!(
            autoren.len() > autoren_now.len(),
            "Expected: {:?}, Got {:?}",
            autoren,
            autoren_now
        );
        let autoren = autoren_now;
        let r = scenario
            .server
            .autoren_delete_by_param(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::KeyAdder, 1),
                &models::AutorenDeleteByParamQueryParams {
                    inifch: None,
                    iniorg: None,
                    inipsn: Some("Harald Maria Töpfer".to_string()),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            r,
            AutorenDeleteByParamResponse::Status204_NoContent {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            }
        );
        let autoren_now = fetch_all_authors(&scenario.server).await;
        assert!(autoren.len() > autoren_now.len());

        scenario.teardown().await;
    }

    async fn enum_delete_with(
        server: &LTZFServer,
        pp: &models::EnumDeletePathParams,
    ) -> crate::Result<EnumDeleteResponse> {
        server
            .enum_delete(
                &Method::DELETE,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::KeyAdder, 1),
                pp,
            )
            .await
    }
    #[tokio::test]
    async fn test_enum_delete() {
        let scenario = TestSetup::new("test_enum_delete").await;
        let r = scenario
            .server
            .enum_delete(
                &Method::DELETE,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::Collector, 1),
                &models::EnumDeletePathParams {
                    item: "absolutely".to_string(),
                    name: models::EnumerationNames::Dokumententypen,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            r,
            EnumDeleteResponse::Status403_Forbidden {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None
            }
        );
        insert_default_vorgang(&scenario.server).await;

        let r = enum_delete_with(
            &scenario.server,
            &models::EnumDeletePathParams {
                item: "preparl-entwurf".to_string(),
                name: models::EnumerationNames::Dokumententypen,
            },
        )
        .await
        .unwrap();
        assert!(matches!(r, EnumDeleteResponse::Status204_NoContent { .. }));

        scenario.teardown().await;
    }

    #[tokio::test]
    async fn test_gremien_delete() {
        let scenario = TestSetup::new("test_gremien_delete").await;
        let r = scenario
            .server
            .gremien_delete_by_param(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::Collector, 1),
                &models::GremienDeleteByParamQueryParams {
                    gr: None,
                    p: None,
                    wp: None,
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            r,
            GremienDeleteByParamResponse::Status403_Forbidden { .. }
        ));

        let mut vorgang = generate::default_vorgang();
        let std_station = generate::default_station();
        vorgang.stationen.push(models::Station {
            api_id: Some(uuid::Uuid::from_str("b18bde64-c0ff-eeee-aaaa-deadbeef106e").unwrap()),
            gremium: Some(models::Gremium {
                link: None,
                name: "abc123".to_string(),
                parlament: models::Parlament::Br,
                wahlperiode: 17,
            }),
            ..std_station.clone()
        });
        vorgang.stationen.push(models::Station {
            api_id: Some(uuid::Uuid::from_str("b18bde64-c0ff-eeee-bbbb-deadbeef106e").unwrap()),
            gremium: Some(models::Gremium {
                link: None,
                name: "rrrrrr".to_string(),
                parlament: models::Parlament::Bt,
                wahlperiode: 12,
            }),
            ..std_station.clone()
        });

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
        assert!(matches!(
            rsp,
            openapi::apis::data_administration_vorgang::VorgangIdPutResponse::Status201_Created { .. }
        ));

        let gremien = fetch_all_gremien(&scenario.server).await;
        let r = scenario
            .server
            .gremien_delete_by_param(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::KeyAdder, 1),
                &models::GremienDeleteByParamQueryParams {
                    gr: Some("abc123".to_string()),
                    p: None,
                    wp: None,
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            r,
            GremienDeleteByParamResponse::Status204_NoContent { .. }
        ));
        let new_gremien = fetch_all_gremien(&scenario.server).await;
        assert!(gremien.len() > new_gremien.len());
        let gremien = new_gremien;

        let r = scenario
            .server
            .gremien_delete_by_param(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::KeyAdder, 1),
                &models::GremienDeleteByParamQueryParams {
                    gr: None,
                    p: Some(models::Parlament::Bt),
                    wp: None,
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            r,
            GremienDeleteByParamResponse::Status204_NoContent { .. }
        ));
        let new_gremien = fetch_all_gremien(&scenario.server).await;
        assert!(gremien.len() > new_gremien.len());
        let gremien = new_gremien;

        let r = scenario
            .server
            .gremien_delete_by_param(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::KeyAdder, 1),
                &models::GremienDeleteByParamQueryParams {
                    gr: None,
                    p: None,
                    wp: Some(20),
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            r,
            GremienDeleteByParamResponse::Status204_NoContent { .. }
        ));
        let new_gremien = fetch_all_gremien(&scenario.server).await;
        assert!(gremien.len() > new_gremien.len());
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn test_enum_put() {
        let scenario = TestSetup::new("test_enum_put").await;
        scenario.teardown().await;
    }
    #[tokio::test]
    async fn test_autor_put() {
        let scenario = TestSetup::new("test_autor_put").await;
        scenario.teardown().await;
    }
    async fn gp_with(
        server: &LTZFServer,
        gpr: &models::GremienPutRequest,
    ) -> crate::Result<GremienPutResponse> {
        server
            .gremien_put(
                &Method::PUT,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::KeyAdder, 1),
                gpr,
            )
            .await
    }
    #[tokio::test]
    async fn test_gremium_put() {
        let scenario = TestSetup::new("test_gremium_put").await;
        // check permissions
        let response = scenario
            .server
            .gremien_put(
                &Method::PUT,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::Collector, 1),
                &models::GremienPutRequest {
                    objects: vec![],
                    replacing: None,
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            response,
            GremienPutResponse::Status403_Forbidden { .. }
        ));
        let other_gremium = models::Gremium {
            link: None,
            name: "Ausschuss für Ware Diggah".to_string(),
            parlament: models::Parlament::Bv,
            wahlperiode: 42,
        };
        // check insert without conflict
        let gremien = fetch_all_gremien(&scenario.server).await;
        let response = gp_with(
            &scenario.server,
            &models::GremienPutRequest {
                objects: vec![other_gremium.clone()],
                replacing: None,
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            response,
            GremienPutResponse::Status201_Created { .. }
        ));
        let gremien_new = fetch_all_gremien(&scenario.server).await;
        assert!(gremien.len() < gremien_new.len());
        assert!(gremien_new.contains(&other_gremium));
        let gremien = gremien_new;

        // check insert with conflict
        let response = gp_with(
            &scenario.server,
            &models::GremienPutRequest {
                objects: vec![other_gremium.clone()],
                replacing: None,
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            response,
            GremienPutResponse::Status304_NotModified { .. }
        ));
        let gremien_new = fetch_all_gremien(&scenario.server).await;
        assert_eq!(gremien.len(), gremien_new.len());
        let gremien = gremien_new;

        // check replace
        let repl_grm = models::Gremium {
            link: None,
            name: "Ausschuss für Ware Diggah2".to_string(),
            parlament: models::Parlament::Bv,
            wahlperiode: 42,
        };
        let response = gp_with(
            &scenario.server,
            &models::GremienPutRequest {
                objects: vec![repl_grm.clone()],
                replacing: Some(vec![models::GremienPutRequestReplacingInner {
                    replaced_by: 0,
                    values: vec![other_gremium.clone()],
                }]),
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            response,
            GremienPutResponse::Status201_Created { .. }
        ));
        let gremien_new = fetch_all_gremien(&scenario.server).await;
        assert_eq!(gremien.len(), gremien_new.len());
        assert!(gremien_new.contains(&repl_grm));

        // malformed request
        let response = gp_with(
            &scenario.server,
            &models::GremienPutRequest {
                objects: vec![models::Gremium {
                    link: None,
                    name: "Ausschuss für Ware Diggah2".to_string(),
                    parlament: models::Parlament::Bv,
                    wahlperiode: 42,
                }],
                replacing: Some(vec![models::GremienPutRequestReplacingInner {
                    replaced_by: 1,
                    values: vec![other_gremium.clone()],
                }]),
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            response,
            GremienPutResponse::Status400_BadRequest { .. }
        ));
        let gremien_new = fetch_all_gremien(&scenario.server).await;
        assert_eq!(gremien.len(), gremien_new.len());

        scenario.teardown().await;
    }
}
