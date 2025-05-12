use crate::{LTZFError, LTZFServer, Result};
use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::CookieJar;
use axum_extra::extract::Host;
use openapi::apis::adminschnittstellen_autoren::*;
use openapi::apis::adminschnittstellen_dokumente::*;
use openapi::apis::adminschnittstellen_enumerations::*;
use openapi::apis::adminschnittstellen_gremien::*;
use openapi::apis::autoren_unauthorisiert::*;
use openapi::apis::dokumente_unauthorisiert::*;
use openapi::apis::enumerations_unauthorisiert::*;
use openapi::apis::gremien_unauthorisiert::*;
use openapi::models;

use super::PaginationResponsePart;

#[async_trait]
impl AutorenUnauthorisiert<LTZFError> for LTZFServer {
    /// AutorenGet - GET /api/v1/autoren
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

        let limit = query_params
            .per_page
            .unwrap_or(PaginationResponsePart::DEFAULT_PER_PAGE);
        let offset = query_params.page.unwrap_or(0) * limit;
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
            limit as i64,
            offset as i64
        )
        .map(|r| models::Autor {
            fachgebiet: r.fachgebiet,
            person: r.person,
            lobbyregister: r.lobbyregister,
            organisation: r.organisation,
        })
        .fetch_all(&mut *tx)
        .await?;
        let prp = PaginationResponsePart::new(
            Some(full_authors),
            query_params.page,
            query_params.per_page,
            "/api/v1/autoren",
        );

        tx.commit().await?;
        return Ok(AutorenGetResponse::Status200_Success {
            body: output,
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
            x_total_count: prp.x_total_count,
            x_total_pages: prp.x_total_pages,
            x_page: prp.x_page,
            x_per_page: prp.x_per_page,
            link: prp.link,
        });
    }
}

#[async_trait]
impl GremienUnauthorisiert<LTZFError> for LTZFServer {
    async fn gremien_get(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        query_params: &models::GremienGetQueryParams,
    ) -> Result<GremienGetResponse> {
        todo!()
    }
}
#[async_trait]
impl EnumerationsUnauthorisiert<LTZFError> for LTZFServer {
    /// EnumGet - GET /api/v1/enumeration/{name}
    async fn enum_get(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EnumGetPathParams,
        query_params: &models::EnumGetQueryParams,
    ) -> Result<EnumGetResponse> {
        todo!()
    }
}
#[async_trait]
impl DokumenteUnauthorisiert<LTZFError> for LTZFServer {
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

#[async_trait]
impl AdminschnittstellenAutoren<LTZFError> for LTZFServer {
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
        body: &Vec<models::Autor>,
    ) -> Result<AutorenPutResponse> {
        todo!()
    }
}

#[async_trait]
impl AdminschnittstellenGremien<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;
    /// GremienDeleteByParam - DELETE /api/v1/gremien
    async fn gremien_delete_by_param(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        query_params: &models::GremienDeleteByParamQueryParams,
    ) -> Result<GremienDeleteByParamResponse> {
        todo!()
    }

    /// GremienPut - PUT /api/v1/gremien
    async fn gremien_put(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        body: &Vec<models::Gremium>,
    ) -> Result<GremienPutResponse> {
        todo!()
    }
}

#[async_trait]
impl AdminschnittstellenEnumerations<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;
    /// EnumDelete - DELETE /api/v1/enumeration/{name}/{item}
    async fn enum_delete(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::EnumDeletePathParams,
    ) -> Result<EnumDeleteResponse> {
        todo!()
    }

    /// EnumPut - PUT /api/v1/enumeration/{name}
    async fn enum_put(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::EnumPutPathParams,
        body: &Vec<models::EnumPutRequestInner>,
    ) -> Result<EnumPutResponse> {
        todo!()
    }
}

#[async_trait]
impl AdminschnittstellenDokumente<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;
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
