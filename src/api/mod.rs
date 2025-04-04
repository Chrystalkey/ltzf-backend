use std::sync::Arc;

use auth::APIScope;
use axum::async_trait;
use axum::extract::Host;
use axum::http::Method;
use axum_extra::extract::cookie::CookieJar;

use crate::db::delete::delete_ass_by_api_id;
use crate::error::{DataValidationError, DatabaseError, LTZFError};
use crate::utils::notify;
use crate::{db, Configuration};

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
            mailbundle: mailbundle.map(|mb| Arc::new(mb)),
        }
    }
}

#[allow(unused_variables)]
#[async_trait]
impl openapi::apis::default::Default for LTZFServer {
    type Claims = (auth::APIScope, i32);

    #[doc = "AuthPost - POST /api/v1/auth"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn auth_post(
        &self,
        method: Method,
        host: Host,
        cookies: CookieJar,
        claims: Self::Claims,
        body: models::CreateApiKey,
    ) -> Result<AuthPostResponse, ()> {
        if claims.0 != auth::APIScope::KeyAdder {
            return Ok(AuthPostResponse::Status401_APIKeyIsMissingOrInvalid);
        }
        let key = auth::auth_get(
            self,
            body.scope.clone().try_into().unwrap(),
            body.expires_at.map(|x| x),
            claims.1,
        )
        .await;
        match key {
            Ok(key) => {
                return Ok(AuthPostResponse::Status201_APIKeyWasCreatedSuccessfully(
                    key,
                ))
            }
            Err(e) => {
                tracing::error!("{}", e.to_string());
                return Err(());
            }
        }
    }

    #[doc = "AuthDelete - DELETE /api/v1/auth"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn auth_delete(
        &self,
        method: Method,
        host: Host,
        cookies: CookieJar,
        claims: Self::Claims,
        header_params: models::AuthDeleteHeaderParams,
    ) -> Result<AuthDeleteResponse, ()> {
        let key_to_delete = &header_params.api_key_delete;
        let ret = auth::auth_delete(self, claims.0, key_to_delete).await;
        match ret {
            Ok(x) => return Ok(x),
            Err(e) => {
                tracing::error!("{}", e.to_string());
                Err(())
            }
        }
    }

    /// KalDateGet - GET /api/v1/kalender/{parlament}/{datum}
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn kal_date_get(
        &self,
        method: Method,
        host: Host,
        cookies: CookieJar,
        header_params: models::KalDateGetHeaderParams,
        path_params: models::KalDateGetPathParams,
    ) -> Result<KalDateGetResponse, ()> {
        let mut tx = self.sqlx_db.begin().await.map_err(|e| {
            tracing::error!("{e}");
        })?;
        let res =
            kalender::kal_get_by_date(path_params.datum, path_params.parlament, &mut tx, self)
                .await
                .map_err(|e| tracing::error!("{e}"))?;
        tx.commit().await.map_err(|e| {
            tracing::error!("{e}");
        })?;
        Ok(res)
    }

    /// KalDatePut - PUT /api/v1/kalender/{parlament}/{datum}
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn kal_date_put(
        &self,
        method: Method,
        host: Host,
        cookies: CookieJar,
        claims: Self::Claims,
        path_params: models::KalDatePutPathParams,
        body: Vec<models::Sitzung>,
    ) -> Result<KalDatePutResponse, ()> {
        let last_upd_day = chrono::Utc::now()
            .date_naive()
            .checked_sub_days(chrono::Days::new(1))
            .unwrap();
        if !(claims.0 == APIScope::Admin
            || claims.0 == APIScope::Collector
            || (claims.0 == APIScope::Collector && path_params.datum > last_upd_day))
        {
            return Ok(KalDatePutResponse::Status401_APIKeyIsMissingOrInvalid);
        }
        let body = body
            .iter()
            .filter(|f| f.termin.date_naive() > last_upd_day)
            .cloned()
            .collect();

        let mut tx = self.sqlx_db.begin().await.map_err(|e| {
            tracing::error!("{e}");
        })?;

        let res = kalender::kal_put_by_date(
            path_params.datum,
            path_params.parlament,
            body,
            &mut tx,
            self,
        )
        .await
        .map_err(|e| tracing::error!("{e}"))?;
        tx.commit().await.map_err(|e| {
            tracing::error!("{e}");
        })?;
        Ok(res)
    }

    /// KalGet - GET /api/v1/kalender
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn kal_get(
        &self,
        method: Method,
        host: Host,
        cookies: CookieJar,
        header_params: models::KalGetHeaderParams,
        query_params: models::KalGetQueryParams,
    ) -> Result<KalGetResponse, ()> {
        let mut tx = self.sqlx_db.begin().await.map_err(|e| {
            tracing::error!("{e}");
        })?;
        let res = kalender::kal_get_by_param(query_params, header_params, &mut tx, self)
            .await
            .map_err(|e| tracing::error!("{e}"))?;
        tx.commit().await.map_err(|e| {
            tracing::error!("{e}");
        })?;
        Ok(res)
    }

    #[doc = "VorgangGetById - GET /api/v1/vorgang/{vorgang_id}"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_get_by_id(
        &self,
        method: Method,
        host: Host,
        cookies: CookieJar,
        header_params: models::VorgangGetByIdHeaderParams,
        path_params: models::VorgangGetByIdPathParams,
    ) -> Result<VorgangGetByIdResponse, ()> {
        let vorgang = objects::vg_id_get(self, &header_params, &path_params).await;

        match vorgang {
            Ok(vorgang) => Ok(VorgangGetByIdResponse::Status200_SuccessfulOperation(
                vorgang,
            )),
            Err(e) => {
                tracing::warn!("{}", e.to_string());
                match e {
                    LTZFError::Database {
                        source:
                            DatabaseError::Sqlx {
                                source: sqlx::Error::RowNotFound,
                            },
                    } => {
                        tracing::warn!("Not Found Error: {:?}", e.to_string());
                        Ok(VorgangGetByIdResponse::Status404_ContentNotFound)
                    }
                    _ => Err(()),
                }
            }
        }
    }
    #[doc = " VorgangDelete - GET /api/v1/vorgang"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_delete(
        &self,
        method: Method,
        host: Host,
        cookies: CookieJar,
        claims: Self::Claims,
        path_params: models::VorgangDeletePathParams,
    ) -> Result<VorgangDeleteResponse, ()> {
        if claims.0 != auth::APIScope::Admin && claims.0 != auth::APIScope::KeyAdder {
            return Ok(VorgangDeleteResponse::Status401_APIKeyIsMissingOrInvalid);
        }
        let api_id = path_params.vorgang_id;
        let result = db::delete::delete_vorgang_by_api_id(api_id, self)
            .await
            .map_err(|e| {
                tracing::warn!("Could not delete Vorgang with ID `{}`: {}", api_id, e);
            });
        return result;
    }
    #[doc = " VorgangIdPut - GET /api/v1/vorgang"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_id_put(
        &self,
        method: Method,
        host: Host,
        cookies: CookieJar,
        claims: Self::Claims,
        path_params: models::VorgangIdPutPathParams,
        body: models::Vorgang,
    ) -> Result<VorgangIdPutResponse, ()> {
        if claims.0 != auth::APIScope::Admin && claims.0 != auth::APIScope::KeyAdder {
            return Ok(VorgangIdPutResponse::Status401_APIKeyIsMissingOrInvalid);
        }
        let out = objects::vorgang_id_put(self, &path_params, &body)
            .await
            .map_err(|e| tracing::warn!("{}", e))?;
        Ok(out)
    }

    #[doc = " VorgangGet - GET /api/v1/vorgang"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_get(
        &self,
        method: Method,
        host: Host,
        cookies: CookieJar,
        header_params: models::VorgangGetHeaderParams,
        query_params: models::VorgangGetQueryParams,
    ) -> Result<VorgangGetResponse, ()> {
        let now = chrono::Utc::now();
        let lower_bnd = header_params.if_modified_since.map(|el| {
            if query_params.since.is_some() {
                query_params.since.unwrap().min(el)
            } else {
                el
            }
        });

        if lower_bnd
            .map(|l| l > now || query_params.until.is_some() && query_params.until.unwrap() < l)
            .unwrap_or(false)
        {
            return Ok(VorgangGetResponse::Status416_RequestRangeNotSatisfiable);
        }
        match objects::vg_get(self, &header_params, &query_params).await {
            Ok(x) => {
                if x.is_empty() {
                    Ok(VorgangGetResponse::Status204_NoContentFoundForTheSpecifiedParameters)
                } else {
                    Ok(VorgangGetResponse::Status200_AntwortAufEineGefilterteAnfrageZuVorgang(x))
                }
            }
            Err(e) => {
                tracing::warn!("{}", e.to_string());
                Err(())
            }
        }
    }

    #[doc = " ApiV1VorgangPost - PUT /api/v1/vorgang"]
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    async fn vorgang_put(
        &self,
        method: Method,
        host: Host,
        cookies: CookieJar,
        claims: Self::Claims,
        query_params: models::VorgangPutQueryParams,
        body: models::Vorgang,
    ) -> Result<VorgangPutResponse, ()> {
        let rval = objects::vorgang_put(self, &body).await;
        match rval {
            Ok(_) => Ok(VorgangPutResponse::Status201_Success),
            Err(e) => {
                tracing::warn!("Error Occurred and Is Returned: {:?}", e.to_string());
                match e {
                    LTZFError::Validation {
                        source: DataValidationError::AmbiguousMatch { .. },
                    } => Ok(VorgangPutResponse::Status409_Conflict),
                    _ => Err(()),
                }
            }
        }
    }
    /// AsDelete - DELETE /api/v1/ausschusssitzung/{as_id}
    async fn sitzung_delete(
        &self,
        method: Method,
        host: Host,
        cookies: CookieJar,
        claims: Self::Claims,
        path_params: models::SitzungDeletePathParams,
    ) -> Result<SitzungDeleteResponse, ()> {
        if claims.0 != auth::APIScope::Admin && claims.0 != auth::APIScope::KeyAdder {
            return Ok(SitzungDeleteResponse::Status401_APIKeyIsMissingOrInvalid);
        }
        Ok(delete_ass_by_api_id(path_params.sid, self)
            .await
            .map_err(|e| {
                tracing::warn!("{}", e);
            })?)
    }

    /// AsGetById - GET /api/v1/ausschusssitzung/{as_id}
    async fn s_get_by_id(
        &self,
        method: Method,
        host: Host,
        cookies: CookieJar,
        header_params: models::SGetByIdHeaderParams,
        path_params: models::SGetByIdPathParams,
    ) -> Result<SGetByIdResponse, ()> {
        let ass = objects::s_get_by_id(&self, &header_params, &path_params)
            .await
            .map_err(|e| {
                tracing::warn!("{}", e);
            })?;
        return Ok(ass);
    }

    /// AsIdPut - PUT /api/v1/ausschusssitzung/{as_id}
    async fn sid_put(
        &self,
        method: Method,
        host: Host,
        cookies: CookieJar,
        claims: Self::Claims,
        path_params: models::SidPutPathParams,
        body: models::Sitzung,
    ) -> Result<SidPutResponse, ()> {
        if claims.0 != auth::APIScope::Admin && claims.0 != auth::APIScope::KeyAdder {
            return Ok(SidPutResponse::Status401_APIKeyIsMissingOrInvalid);
        }
        let out = objects::s_id_put(self, &path_params, &body)
            .await
            .map_err(|e| {
                tracing::warn!("{}", e);
            })?;
        Ok(out)
    }

    /// AsGet - GET /api/v1/ausschusssitzung
    async fn s_get(
        &self,
        method: Method,
        host: Host,
        cookies: CookieJar,
        header_params: models::SGetHeaderParams,
        query_params: models::SGetQueryParams,
    ) -> Result<SGetResponse, ()> {
        let res = objects::s_get(self, &query_params, &header_params)
            .await
            .map_err(|e| tracing::error!("{e}"))?;
        Ok(res)
    }
}
