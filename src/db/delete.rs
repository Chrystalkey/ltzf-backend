use crate::{LTZFServer, Result};
use openapi::apis::data_administration_sitzung::*;
use openapi::apis::data_administration_vorgang::*;
use uuid::Uuid;

pub async fn delete_vorgang_by_api_id(
    api_id: Uuid,
    server: &LTZFServer,
) -> Result<VorgangDeleteResponse> {
    let thing = sqlx::query!("SELECT 1 as x FROM vorgang WHERE api_id = $1", api_id)
        .fetch_optional(&server.sqlx_db)
        .await?;
    if thing.is_none() {
        return Ok(VorgangDeleteResponse::Status404_NotFound {
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
        });
    }
    sqlx::query!("DELETE FROM vorgang WHERE api_id = $1", api_id)
        .execute(&server.sqlx_db)
        .await?;

    Ok(VorgangDeleteResponse::Status204_NoContent {
        x_rate_limit_limit: None,
        x_rate_limit_remaining: None,
        x_rate_limit_reset: None,
    })
}
pub async fn delete_sitzung_by_api_id(
    api_id: Uuid,
    server: &LTZFServer,
) -> Result<SitzungDeleteResponse> {
    let thing = sqlx::query!("SELECT 1 as x FROM sitzung WHERE api_id = $1", api_id)
        .fetch_optional(&server.sqlx_db)
        .await?;
    if thing.is_none() {
        return Ok(SitzungDeleteResponse::Status404_NotFound {
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
        });
    }
    sqlx::query!("DELETE FROM sitzung WHERE api_id = $1", api_id)
        .execute(&server.sqlx_db)
        .await?;
    Ok(SitzungDeleteResponse::Status204_NoContent {
        x_rate_limit_limit: None,
        x_rate_limit_remaining: None,
        x_rate_limit_reset: None,
    })
}
