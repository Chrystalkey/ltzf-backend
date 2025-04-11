use std::sync::Arc;

use async_trait::async_trait;
use auth::APIScope;
use axum::http::Method;
use axum_extra::extract::{Host, cookie::CookieJar};

use crate::Result;
use crate::db::delete::delete_ass_by_api_id;
use crate::error::{DataValidationError, LTZFError};
use crate::utils::notify;
use crate::{Configuration, db};

use openapi::apis::default::*;
use openapi::models;

mod auth;
mod kalender;
mod objects;

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

#[allow(unused_variables)]
#[async_trait]
impl openapi::apis::default::Default<LTZFError> for LTZFServer {
    type Claims = (auth::APIScope, i32);

    #[doc = "AuthPost - POST /api/v1/auth"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn auth_post(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        body: &models::CreateApiKey,
    ) -> Result<AuthPostResponse> {
        if claims.0 != auth::APIScope::KeyAdder {
            return Ok(AuthPostResponse::Status401_APIKeyIsMissingOrInvalid);
        }
        match auth::auth_get(
            self,
            body.scope.clone().try_into().unwrap(),
            body.expires_at,
            claims.1,
        )
        .await
        {
            Ok(key) => {
                return Ok(AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(
                    key,
                ));
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    #[doc = "AuthDelete - DELETE /api/v1/auth"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn auth_delete(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        header_params: &models::AuthDeleteHeaderParams,
    ) -> Result<AuthDeleteResponse> {
        if claims.0 != APIScope::KeyAdder {
            return Ok(
                openapi::apis::default::AuthDeleteResponse::Status401_APIKeyIsMissingOrInvalid,
            );
        }
        let key_to_delete = &header_params.api_key_delete;
        return auth::auth_delete(self, key_to_delete).await;
    }

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
    ) -> Result<KalDateGetResponse> {
        let mut tx = self.sqlx_db.begin().await?;
        let res =
            kalender::kal_get_by_date(path_params.datum, path_params.parlament, &mut tx, self)
                .await?;
        tx.commit().await?;
        Ok(res)
    }

    #[doc = "KalDatePut - PUT /api/v1/kalender/{parlament}/{datum}"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn kal_date_put(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
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
            return Ok(KalDatePutResponse::Status401_APIKeyIsMissingOrInvalid);
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

        let res = kalender::kal_put_by_date(
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
        let res = kalender::kal_get_by_param(query_params, header_params, &mut tx, self).await?;
        tx.commit().await?;
        Ok(res)
    }

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
        let vorgang = objects::vg_id_get(self, header_params, path_params).await;

        match vorgang {
            Ok(vorgang) => Ok(VorgangGetByIdResponse::Status200_SuccessfulOperation(
                vorgang,
            )),
            Err(e) => match e {
                LTZFError::Validation {
                    source: crate::error::DataValidationError::QueryParametersNotSatisfied,
                } => Ok(VorgangGetByIdResponse::Status304_NoNewChanges),
                LTZFError::Database {
                    source:
                        crate::error::DatabaseError::Sqlx {
                            source: sqlx::Error::RowNotFound,
                        },
                } => Ok(VorgangGetByIdResponse::Status404_ContentNotFound),
                _ => Err(e),
            },
        }
    }
    #[doc = " VorgangDelete - GET /api/v1/vorgang"]
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
        let api_id = path_params.vorgang_id;
        let result = db::delete::delete_vorgang_by_api_id(api_id, self).await?;
        return Ok(result);
    }
    #[doc = " VorgangIdPut - GET /api/v1/vorgang"]
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
            return Ok(VorgangIdPutResponse::Status401_APIKeyIsMissingOrInvalid);
        }
        let out = objects::vorgang_id_put(self, path_params, body).await?;
        Ok(out)
    }

    #[doc = " VorgangGet - GET /api/v1/vorgang"]
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
        match objects::vg_get(header_params, query_params, &mut tx).await {
            Ok(x) => {
                tx.commit().await?;
                Ok(x)
            }
            Err(e) => Err(e),
        }
    }

    #[doc = " ApiV1VorgangPost - PUT /api/v1/vorgang"]
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
            return Ok(VorgangPutResponse::Status401_APIKeyIsMissingOrInvalid);
        }
        let rval = objects::vorgang_put(self, body).await;
        match rval {
            Ok(_) => Ok(VorgangPutResponse::Status201_Success),
            Err(e) => match e {
                LTZFError::Validation {
                    source: DataValidationError::AmbiguousMatch { .. },
                } => Ok(VorgangPutResponse::Status409_Conflict),
                _ => Err(e),
            },
        }
    }

    #[doc = " sitzung_delete - PUT /api/v1/vorgang"]
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
            return Ok(SitzungDeleteResponse::Status401_APIKeyIsMissingOrInvalid);
        }
        Ok(delete_ass_by_api_id(path_params.sid, self).await?)
    }

    #[doc = " SGetById - PUT /api/v1/sitzung/{sid}"]
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

    #[doc = " sid_put - PUT /api/v1/sitzung/{sid}"]
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
            return Ok(SidPutResponse::Status401_APIKeyIsMissingOrInvalid);
        }
        let out = objects::s_id_put(self, path_params, body).await?;
        Ok(out)
    }

    #[doc = " SGet - GET /api/v1/sitzung"]
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
