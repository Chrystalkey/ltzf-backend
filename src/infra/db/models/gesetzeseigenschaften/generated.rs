/* This file is generated and managed by dsync */

use crate::diesel::*;
use crate::schema::*;
use diesel::QueryResult;
use serde::{Deserialize, Serialize};


type Connection = diesel::r2d2::PooledConnection<diesel::r2d2::ConnectionManager<diesel::PgConnection>>;

#[derive(Debug, Serialize, Deserialize, Clone, Queryable, Insertable, AsChangeset, Selectable)]
#[diesel(table_name=gesetzeseigenschaften, primary_key(id))]
pub struct Gesetzeseigenschaften {
    pub id: i32,
    pub eigenschaft: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Queryable, Insertable, AsChangeset)]
#[diesel(table_name=gesetzeseigenschaften)]
pub struct CreateGesetzeseigenschaften {
    pub id: i32,
    pub eigenschaft: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Queryable, Insertable, AsChangeset)]
#[diesel(table_name=gesetzeseigenschaften)]
pub struct UpdateGesetzeseigenschaften {
    pub eigenschaft: Option<String>,
}


#[derive(Debug, Serialize)]
pub struct PaginationResult<T> {
    pub items: Vec<T>,
    pub total_items: i64,
    /// 0-based index
    pub page: i64,
    pub page_size: i64,
    pub num_pages: i64,
}

impl Gesetzeseigenschaften {

    pub fn create(db: &mut Connection, item: &CreateGesetzeseigenschaften) -> QueryResult<Self> {
        use crate::schema::gesetzeseigenschaften::dsl::*;

        insert_into(gesetzeseigenschaften).values(item).get_result::<Self>(db)
    }

    pub fn read(db: &mut Connection, param_id: i32) -> QueryResult<Self> {
        use crate::schema::gesetzeseigenschaften::dsl::*;

        gesetzeseigenschaften.filter(id.eq(param_id)).first::<Self>(db)
    }

    /// Paginates through the table where page is a 0-based index (i.e. page 0 is the first page)
    pub fn paginate(db: &mut Connection, page: i64, page_size: i64) -> QueryResult<PaginationResult<Self>> {
        use crate::schema::gesetzeseigenschaften::dsl::*;

        let page_size = if page_size < 1 { 1 } else { page_size };
        let total_items = gesetzeseigenschaften.count().get_result(db)?;
        let items = gesetzeseigenschaften.limit(page_size).offset(page * page_size).load::<Self>(db)?;

        Ok(PaginationResult {
            items,
            total_items,
            page,
            page_size,
            /* ceiling division of integers */
            num_pages: total_items / page_size + i64::from(total_items % page_size != 0)
        })
    }

    pub fn update(db: &mut Connection, param_id: i32, item: &UpdateGesetzeseigenschaften) -> QueryResult<Self> {
        use crate::schema::gesetzeseigenschaften::dsl::*;

        diesel::update(gesetzeseigenschaften.filter(id.eq(param_id))).set(item).get_result(db)
    }

    pub fn delete(db: &mut Connection, param_id: i32) -> QueryResult<usize> {
        use crate::schema::gesetzeseigenschaften::dsl::*;

        diesel::delete(gesetzeseigenschaften.filter(id.eq(param_id))).execute(db)
    }

}