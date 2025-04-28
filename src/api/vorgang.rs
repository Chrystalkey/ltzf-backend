use crate::db::{delete, insert, merge, retrieve};
use crate::{LTZFServer, Result};
use async_trait::async_trait;
use openapi::{
    apis::adminschnittstellen_vorgnge::*, apis::collector_schnittstellen_vorgnge::*,
    apis::unauthorisiert_vorgnge::*, models,
};

use super::compare::*;
use super::sitzung::find_applicable_date_range;

#[async_trait]
impl AdminschnittstellenVorgnge<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;
    #[doc = "VorgangDelete - DELETE /api/v1/vorgang/{vorgang_id}"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_delete(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::VorgangDeletePathParams,
    ) -> Result<VorgangDeleteResponse> {
        if claims.0 != auth::APIScope::Admin && claims.0 != auth::APIScope::KeyAdder {
            return Ok(VorgangDeleteResponse::Status401_APIKeyIsMissingOrInvalid);
        }
        db::delete::delete_vorgang_by_api_id(path_params.vorgang_id, self).await
    }

    #[doc = "VorgangIdPut - PUT /api/v1/vorgang/{vorgang_id}"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_id_put(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::VorgangIdPutPathParams,
        body: &models::Vorgang,
    ) -> Result<VorgangIdPutResponse> {
        if claims.0 != auth::APIScope::Admin && claims.0 != auth::APIScope::KeyAdder {
            return Ok(VorgangIdPutResponse::Status403_AuthenticationFailed { x_rate_limit_limit: (), x_rate_limit_remaining: (), x_rate_limit_reset: () });
        }
        let mut tx = server.sqlx_db.begin().await?;
        let api_id = path_params.vorgang_id;
        let db_id = sqlx::query!("SELECT id FROM vorgang WHERE api_id = $1", api_id)
            .map(|x| x.id)
            .fetch_optional(&mut *tx)
            .await?;
        match db_id {
            Some(db_id) => {
                let db_cmpvg = retrieve::vorgang_by_id(db_id, &mut tx).await?;
                if compare_vorgang(&db_cmpvg, body) {
                    return Ok(VorgangIdPutResponse::Status304_NotModified { x_rate_limit_limit: (), x_rate_limit_remaining: (), x_rate_limit_reset: () });
                }
                match delete::delete_vorgang_by_api_id(api_id, server).await? {
                    openapi::apis::default::VorgangDeleteResponse::Status204_DeletedSuccessfully => {
                        insert::insert_vorgang(body, &mut tx, server).await?;
                    }
                    _ => {
                        unreachable!("If this is reached, some assumptions did not hold")
                    }
                }
            }
            None => {
                insert::insert_vorgang(body, &mut tx, server).await?;
            }
        }
        tx.commit().await?;
        Ok(VorgangIdPutResponse::Status201_Created)
    }
}

#[async_trait]
impl CollectorSchnittstellenVorgnge<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;

    #[doc = "VorgangPut - PUT /api/v1/vorgang"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_put(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        query_params: &models::VorgangPutQueryParams,
        body: &models::Vorgang,
    ) -> Result<VorgangPutResponse> {
        // technically not necessary since all authenticated scopes are allowed, still, better be explicit about that
        if claims.0 != APIScope::KeyAdder
            && claims.0 != APIScope::Admin
            && claims.0 != APIScope::Collector
        {
            return Ok(VorgangPutResponse::Status403_AuthenticationFailed { x_rate_limit_limit: (), x_rate_limit_remaining: (), x_rate_limit_reset: () });
        }
        let rval = merge::vorgang::run_integration(body, server).await;
        match rval {
            Ok(_) => Ok(VorgangPutResponse::Status201_Created { x_rate_limit_limit: (), x_rate_limit_remaining: (), x_rate_limit_reset: () }),
            Err(e) => match &e {
                LTZFError::Validation { source } => match **source {
                    DataValidationError::AmbiguousMatch { .. } => {
                        Ok(VorgangPutResponse::Status409_Conflict { x_rate_limit_limit: (), x_rate_limit_remaining: (), x_rate_limit_reset: () })
                    }
                    _ => Err(e),
                },
                _ => Err(e),
            },
        }
    }
}

#[async_trait]
impl UnauthorisiertVorgnge<LTZFError> for LTZFServer {
    #[doc = "VorgangGetById - GET /api/v1/vorgang/{vorgang_id}"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_get_by_id(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        header_params: &models::VorgangGetByIdHeaderParams,
        path_params: &models::VorgangGetByIdPathParams,
    ) -> Result<VorgangGetByIdResponse> {
        tracing::trace!(
            "vorgang_get_by_id called with id {}",
            path_params.vorgang_id
        );
        let mut tx = server.sqlx_db.begin().await?;
        let exists = sqlx::query!(
            "SELECT 1 as out FROM vorgang WHERE api_id = $1",
            path_params.vorgang_id
        )
        .fetch_one(&mut *tx)
        .await?;
        if !exists{
            return Ok(VorgangGetByIdResponse::Status404_NotFound { x_rate_limit_limit: (), x_rate_limit_remaining: (), x_rate_limit_reset: () });
        }
        let dbid = sqlx::query!(
            "SELECT id FROM vorgang WHERE api_id = $1 AND EXISTS (
                SELECT 1 FROM station s WHERE s.zp_modifiziert > COALESCE($2, CAST('1940-01-01T00:00:00Z' AS TIMESTAMPTZ)) AND s.vg_id = vorgang.id
            )",
            path_params.vorgang_id,
            header_params.if_modified_since
        )
        .map(|x| x.id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(dbid) = dbid {
            let result = retrieve::vorgang_by_id(dbid, &mut tx).await?;
            tx.commit().await?;
            Ok(VorgangGetByIdResponse::Status200_Success { body: vorgang, x_rate_limit_limit: (), x_rate_limit_remaining: (), x_rate_limit_reset: () })
        } else {
            return Ok(VorgangGetByIdResponse::Status304_NotModified { x_rate_limit_limit: (), x_rate_limit_remaining: (), x_rate_limit_reset: () });
        }
    }

    #[doc = "VorgangGet - GET /api/v1/vorgang"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_get(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
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
                limit: query_params.limit,
                offset: query_params.offset,
                lower_date: range.since,
                parlament: query_params.p,
                upper_date: range.until,
                vgtyp: query_params.vgtyp,
                wp: query_params.wp,
                inifch: query_params.inifch.clone(),
                iniorg: query_params.iniorg.clone(),
                inipsn: query_params.inipsn.clone(),
            };
            let result = retrieve::vorgang_by_parameter(parameters, tx).await?;
            if result.is_empty() && header_params.if_modified_since.is_none() {
                tx.rollback().await?;
                Ok(VorgangGetResponse::Status204_NoContent { x_rate_limit_limit: (), x_rate_limit_remaining: (), x_rate_limit_reset: () })
            } else if result.is_empty() && header_params.if_modified_since.is_some() {
                tx.rollback().await?;
                Ok(VorgangGetResponse::Status304_NotModified { x_rate_limit_limit: (), x_rate_limit_remaining: (), x_rate_limit_reset: () })
            } else {
                tx.commit().await?;
                Ok(VorgangGetResponse::Status200_SuccessfulResponseContainingAListOfLegislativeProcessesMatchingTheQueryFilters { body: result, x_total_count: (), x_total_pages: (), x_page: (), x_per_page: (), x_rate_limit_limit: (), x_rate_limit_remaining: (), x_rate_limit_reset: () })
            }
        } else {
            tx.rollback().await?;
            Ok(VorgangGetResponse::Status416_RequestRangeNotSatisfiable { x_rate_limit_limit: (), x_rate_limit_remaining: (), x_rate_limit_reset: () })
        }
    }
}

