use crate::db::{delete, insert, merge, retrieve};
use crate::{LTZFServer, Result};
use openapi::{apis::adminschnittstellen_vorgnge::VorgangIdPutResponse, models};

use super::compare::*;
use super::sitzung::find_applicable_date_range;

pub async fn vg_id_get(
    server: &LTZFServer,
    header_params: &models::VorgangGetByIdHeaderParams,
    path_params: &models::VorgangGetByIdPathParams,
) -> Result<models::Vorgang> {
    let mut tx = server.sqlx_db.begin().await?;
    let _exists = sqlx::query!(
        "SELECT 1 as out FROM vorgang WHERE api_id = $1",
        path_params.vorgang_id
    )
    .fetch_one(&mut *tx)
    .await?;
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
        Ok(result)
    } else {
        Err(crate::error::LTZFError::Validation {
            source: Box::new(crate::error::DataValidationError::QueryParametersNotSatisfied),
        })
    }
}
pub async fn vg_get(
    header_params: &models::VorgangGetHeaderParams,
    query_params: &models::VorgangGetQueryParams,
    tx: &mut sqlx::PgTransaction<'_>,
) -> Result<openapi::apis::default::VorgangGetResponse> {
    use openapi::apis::default::VorgangGetResponse;
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
            Ok(VorgangGetResponse::Status204_NoContentFoundForTheSpecifiedParameters)
        } else if result.is_empty() && header_params.if_modified_since.is_some() {
            Ok(VorgangGetResponse::Status304_NoNewChanges)
        } else {
            Ok(VorgangGetResponse::Status200_AntwortAufEineGefilterteAnfrageZuVorgang(result))
        }
    } else {
        Ok(VorgangGetResponse::Status416_RequestRangeNotSatisfiable)
    }
}

pub async fn vorgang_id_put(
    server: &LTZFServer,
    path_params: &models::VorgangIdPutPathParams,
    body: &models::Vorgang,
) -> Result<VorgangIdPutResponse> {
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
                return Ok(VorgangIdPutResponse::Status204_ContentUnchanged);
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

pub async fn vorgang_put(server: &LTZFServer, model: &models::Vorgang) -> Result<()> {
    merge::vorgang::run_integration(model, server).await?;
    Ok(())
}
