use crate::{LTZFServer, Result};
use openapi::apis::default::{SitzungDeleteResponse, VorgangDeleteResponse};
use uuid::Uuid;

pub async fn delete_vorgang_by_api_id(
    api_id: Uuid,
    server: &LTZFServer,
) -> Result<VorgangDeleteResponse> {
    let thing = sqlx::query!("SELECT * FROM vorgang WHERE api_id = $1", api_id)
        .fetch_optional(&server.sqlx_db)
        .await?;
    if thing.is_none() {
        return Ok(VorgangDeleteResponse::Status404_NoElementWithThisID);
    }
    sqlx::query!("DELETE FROM vorgang WHERE api_id = $1", api_id)
        .execute(&server.sqlx_db)
        .await?;

    Ok(VorgangDeleteResponse::Status204_DeletedSuccessfully)
}
pub async fn delete_sitzung_by_api_id(
    api_id: Uuid,
    server: &LTZFServer,
) -> Result<SitzungDeleteResponse> {
    let thing = sqlx::query!("SELECT id FROM sitzung WHERE api_id = $1", api_id)
        .fetch_optional(&server.sqlx_db)
        .await?;
    if thing.is_none() {
        return Ok(SitzungDeleteResponse::Status404_NoElementWithThisID);
    }
    sqlx::query!("DELETE FROM sitzung WHERE api_id = $1", api_id)
        .execute(&server.sqlx_db)
        .await?;
    Ok(SitzungDeleteResponse::Status204_DeletedSuccessfully)
}
