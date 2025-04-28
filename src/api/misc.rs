use crate::Result;
use async_trait::async_trait;
use openapi::apis::adminschnittstellen_autoren::*;
use openapi::apis::adminschnittstellen_dokumente::*;
use openapi::apis::adminschnittstellen_enumerations::*;
use openapi::apis::adminschnittstellen_gremien::*;
use openapi::apis::autoren_unauthorisiert::*;
use openapi::apis::dokumente_unauthorisiert::*;
use openapi::apis::enumerations_unauthorisiert::*;
use openapi::apis::gremien_unauthorisiert::*;

#[async_trait]
impl AutorenUnauthorisiert<LTZFError> for LTZFServer {
    /// AutorenGet - GET /api/v1/autoren
    async fn autoren_get(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        query_params: &models::AutorenGetQueryParams,
    ) -> Result<AutorenGetResponse> {
        todo!()
    }
}

#[async_trait]
impl GremienUnauthorisiert<LTZFError> for LTZFServer {
    async fn gremien_get(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
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
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::DokumentGetByIdPathParams,
    ) -> Result<DokumentGetByIdResponse> {
        todo!()
    }
}
#[async_trait]
impl AdminschnittstellenAutoren<LTZFError> for LTZFServer {
    type Claims = crate::api::Claims;
    /// AutorenDeleteByParam - DELETE /api/v1/autoren
    async fn autoren_delete_by_param(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        query_params: &models::AutorenDeleteByParamQueryParams,
    ) -> Result<AutorenDeleteByParamResponse> {
        todo!()
    }

    /// AutorenPut - PUT /api/v1/autoren
    async fn autoren_put(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
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
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::DokumentDeleteIdPathParams,
    ) -> Result<DokumentDeleteIdResponse> {
        todo!()
    }

    /// DokumentPutId - PUT /api/v1/dokument/{api_id}
    async fn dokument_put_id(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        claims: &Self::Claims,
        path_params: &models::DokumentPutIdPathParams,
        body: &models::Dokument,
    ) -> Result<DokumentPutIdResponse> {
        todo!()
    }
}
