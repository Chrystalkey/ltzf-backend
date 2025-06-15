use std::collections::BTreeMap;

use crate::api::auth::APIScope;
use crate::db::retrieve::{authors_all_exist, gremien_all_exist};
use crate::{LTZFError, LTZFServer, Result};
use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::CookieJar;
use axum_extra::extract::Host;
use openapi::apis::data_administration_miscellaneous::*;
use openapi::models;
use sqlx::Row;

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
        let enum_tables = std::collections::BTreeMap::from_iter(
            vec![
                (Schlagworte, "schlagwort"),
                (Stationstypen, "stationstyp"),
                (Parlamente, "parlament"),
                (Vorgangstypen, "vorgangstyp"),
                (Dokumententypen, "dokumententyp"),
                (Vgidtypen, "vg_ident_typ"),
            ]
            .drain(..),
        );
        sqlx::query(&format!(
            "DELETE FROM {} x WHERE x.value = $1",
            enum_tables[&path_params.name]
        ))
        .bind::<_>(&path_params.item)
        .execute(&mut *tx)
        .await?;
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
        // if so: Bad Request
        if let Some(replc) = &body.replacing {
            for rpl in replc.iter() {
                if rpl.replaced_by as usize >= body.objects.len() {
                    return Ok(AutorenPutResponse::Status400_BadRequest {
                        x_rate_limit_limit: None,
                        x_rate_limit_remaining: None,
                        x_rate_limit_reset: None,
                    });
                }
            }
        }
        let mut tx = self.sqlx_db.begin().await?;
        // check if all authors are existent in the database
        // check if none of the replacing authors are in the database
        // if both: NotModified
        let (mut person, mut organisation, mut fach, mut lobby) = (vec![], vec![], vec![], vec![]);
        for a in body.objects.iter() {
            person.push(a.person.clone());
            organisation.push(a.organisation.clone());
            fach.push(a.fachgebiet.clone());
            lobby.push(a.lobbyregister.clone());
        }

        if authors_all_exist(&mut tx, &body.objects).await? {
            // flatten the replacement objects and check for existence
            if let Some(repl) = &body.replacing {
                let flattened: Vec<models::Autor> =
                    repl.iter().flat_map(|o| o.values.iter()).cloned().collect();
                if authors_all_exist(&mut tx, &flattened).await? {
                    return Ok(AutorenPutResponse::Status304_NotModified {
                        x_rate_limit_limit: None,
                        x_rate_limit_remaining: None,
                        x_rate_limit_reset: None,
                    });
                }
            } else {
                return Ok(AutorenPutResponse::Status304_NotModified {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None,
                });
            }
        }

        // insert all authors, fetch their IDs
        let new_ids = sqlx::query!("
        INSERT INTO autor(person, organisation, fachgebiet, lobbyregister) 

        SELECT ps, og, fc, lb FROM UNNEST($1::text[], $2::text[], $3::text[], $4::text[]) AS iv(ps, og, fc, lb)

        ON CONFLICT ON CONSTRAINT unq_data 
        DO UPDATE SET 
        fachgebiet = EXCLUDED.fachgebiet,
        lobbyregister = EXCLUDED.lobbyregister

        RETURNING autor.id
        ", &person[..] as &[Option<String>], &organisation[..], &fach[..] as &[Option<String>], &lobby[..] as &[Option<String>])
        .map(|r| r.id)
        .fetch_all(&mut *tx).await?;

        if body.replacing.is_none() {
            tx.commit().await?;
            // if there is nothing to replace, we are done here
            // CAREFUL: HERE DANGLING AUTHOR ENTRIES ARE CREATED
            return Ok(AutorenPutResponse::Status201_Created {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        // for each replacing autor:
        // for each table referencing it: Update those tables with the new id
        let mut replacement_tuples = vec![];
        for entry in body.replacing.as_ref().unwrap().iter() {
            let (mut vperson, mut vorga) = (vec![], vec![]);
            for value in entry.values.iter() {
                vperson.push(value.person.clone());
                vorga.push(value.organisation.clone());
            }
            let value_ids: Vec<_> = sqlx::query!(
                "SELECT $3::int4 as repl_with, a.id as origin FROM
                UNNEST($1::text[], $2::text[]) as iv(ps, og)
                INNER JOIN autor a ON 
                (a.person IS NULL AND iv.ps IS NULL OR a.person=iv.ps) AND 
                a.organisation = iv.og",
                &vperson[..] as &[Option<String>],
                &vorga[..],
                entry.replaced_by as i32
            )
            .map(|r| (new_ids[r.repl_with.unwrap() as usize], r.origin))
            .fetch_all(&mut *tx)
            .await?;
            replacement_tuples.extend(value_ids);
        }
        let rep_new: Vec<_> = replacement_tuples.iter().map(|x| x.0).collect();
        let rep_old: Vec<_> = replacement_tuples.iter().map(|x| x.1).collect();

        // tables referencing authors:
        let tables = vec![
            ("rel_dok_autor", "aut_id"),
            ("rel_vorgang_init", "in_id"),
            ("rel_sitzung_experten", "eid"),
            ("lobbyregistereintrag", "organisation"),
        ];
        for (table, column) in tables {
            sqlx::query(&format!(
                "
            WITH lookup AS (SELECT * FROM UNNEST($1::int4[], $2::int4[]) AS la(new, old))
            UPDATE {table} 
            SET {column} = (SELECT new FROM lookup WHERE old={column})
            WHERE {column} = ANY($2::int4[])
            "
            ))
            .bind(&rep_new[..])
            .bind(&rep_old[..])
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query!(
            "DELETE FROM autor a WHERE a.id = ANY($1::int4[])",
            &rep_old[..]
        )
        .execute(&mut *tx)
        .await?;

        // return 201Created
        tx.commit().await?;
        Ok(AutorenPutResponse::Status201_Created {
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
        })
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
        // check if none of the replacing gremien are in the database or replacing is None
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
                let flattened: Vec<models::Gremium> =
                    repl.iter().flat_map(|o| o.values.iter()).cloned().collect();
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
        // tables that reference a gremium:
        // - station(gr_id)
        // - sitzung(gr_id)
        let tables = vec![("station", "gr_id"), ("sitzung", "gr_id")];
        for (table, column) in tables {
            sqlx::query(&format!(
                "
            WITH lookup AS (SELECT * FROM UNNEST($1::int4[], $2::int4[]) AS la(new, old))
            UPDATE {table} 
            SET {column} = (SELECT new FROM lookup WHERE old={column})
            WHERE {column} = ANY($2::int4[])"
            ))
            .bind(&rep_new[..])
            .bind(&rep_old[..])
            .execute(&mut *tx)
            .await?;
        }
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
        // check if replacing contain an index larger than the object list
        // if so: Bad Request
        if let Some(replc) = &body.replacing {
            for rpl in replc.iter() {
                if rpl.replaced_by as usize >= body.objects.len() {
                    return Ok(EnumPutResponse::Status400_BadRequest {
                        x_rate_limit_limit: None,
                        x_rate_limit_remaining: None,
                        x_rate_limit_reset: None,
                    });
                }
            }
        }
        let mut tx = self.sqlx_db.begin().await?;
        // check if all gremien are existent in the database
        // check if none of the replacing gremien are in the database or replacing is None
        // if both: NotModified
        let enum_tables = std::collections::BTreeMap::from_iter(
            vec![
                (models::EnumerationNames::Schlagworte, "schlagwort"),
                (models::EnumerationNames::Stationstypen, "stationstyp"),
                (models::EnumerationNames::Parlamente, "parlament"),
                (models::EnumerationNames::Vorgangstypen, "vorgangstyp"),
                (models::EnumerationNames::Dokumententypen, "dokumententyp"),
                (models::EnumerationNames::Vgidtypen, "vg_ident_typ"),
            ]
            .drain(..),
        );

        let present = sqlx::query(&format!(
            "SELECT COUNT(1) as cnt FROM UNNEST($1::text[]) as item WHERE EXISTS(SELECT 1 FROM {} x WHERE item=x.value)",
            enum_tables[&path_params.name]
        )).bind(&body.objects[..])
        .map(|r| r.get::<i64, _>(0) as usize)
        .fetch_one(&mut *tx).await?;

        if present == body.objects.len() {
            // flatten the replacement objects and check for existence
            if let Some(repl) = &body.replacing {
                let flattened: Vec<String> =
                    repl.iter().flat_map(|o| o.values.iter()).cloned().collect();
                let present = sqlx::query(&format!(
                    "SELECT COUNT(1) FROM UNNEST($1::text[]) as item WHERE EXISTS(SELECT 1 FROM {} x WHERE item=x.value)",
                    enum_tables[&path_params.name]
                )).bind(&flattened[..])
                .map(|r| r.get::<i64, _>(0) as usize)
                .fetch_one(&mut *tx).await?;

                if present == flattened.len() {
                    return Ok(EnumPutResponse::Status304_NotModified {
                        x_rate_limit_limit: None,
                        x_rate_limit_remaining: None,
                        x_rate_limit_reset: None,
                    });
                }
            } else {
                return Ok(EnumPutResponse::Status304_NotModified {
                    x_rate_limit_limit: None,
                    x_rate_limit_remaining: None,
                    x_rate_limit_reset: None,
                });
            }
        }

        // insert all gremien, fetch their IDs
        let new_ids = sqlx::query(&format!(
            "INSERT INTO {} (value)
                SELECT item FROM UNNEST($1::text[]) as item 
                ON CONFLICT(value) DO UPDATE SET value=EXCLUDED.value
                RETURNING id",
            enum_tables[&path_params.name]
        ))
        .bind(&body.objects[..])
        .map(|r| r.get::<i32, _>(0))
        .fetch_all(&mut *tx)
        .await?;

        if body.replacing.is_none() {
            tx.commit().await?;
            // if there is nothing to replace, we are done here
            // CAREFUL: HERE DANGLING GREMIUM ENTRIES ARE CREATED
            return Ok(EnumPutResponse::Status201_Created {
                x_rate_limit_limit: None,
                x_rate_limit_remaining: None,
                x_rate_limit_reset: None,
            });
        }
        // for each replacing gremium:
        // for each table referencing it: Update those tables with the new id
        let mut replacement_tuples = vec![];
        for entry in body.replacing.as_ref().unwrap().iter() {
            let vitems: Vec<String> = entry.values.clone();
            let value_ids: Vec<_> = sqlx::query(&format!(
                "SELECT $2::int4 as repl_with, x.id as origin FROM
                UNNEST($1::text[]) as item
                INNER JOIN {} x ON x.value = item",
                enum_tables[&path_params.name]
            ))
            .bind(&vitems[..])
            .bind(new_ids[entry.replaced_by as usize] as i32)
            .map(|r| (r.get::<i32, _>(0), r.get::<i32, _>(1)))
            .fetch_all(&mut *tx)
            .await?;
            replacement_tuples.extend(value_ids);
        }
        let rep_new: Vec<_> = replacement_tuples.iter().map(|x| x.0).collect();
        let rep_old: Vec<_> = replacement_tuples.iter().map(|x| x.1).collect();
        // referencing tables:
        // parlament: gremium(parl) / station(p_id)
        // dokumententyp: dokument(typ)
        // stationstyp: station(typ)
        // vg_ident_typ: rel_vorgang_ident(typ)
        // vorgangstyp: vorgang(typ)
        // schlagwort: rel_station_schlagwort(sw_id) / rel_dok_schlagwort(sw_id)
        let enum_table_refs = BTreeMap::from_iter(
            vec![
                (
                    models::EnumerationNames::Parlamente,
                    BTreeMap::from_iter(vec![("gremium", "parl"), ("station", "p_id")].drain(..)),
                ),
                (
                    models::EnumerationNames::Dokumententypen,
                    BTreeMap::from_iter(vec![("dokument", "typ")].drain(..)),
                ),
                (
                    models::EnumerationNames::Stationstypen,
                    BTreeMap::from_iter(vec![("station", "typ")].drain(..)),
                ),
                (
                    models::EnumerationNames::Vgidtypen,
                    BTreeMap::from_iter(vec![("rel_vorgang_ident", "typ")].drain(..)),
                ),
                (
                    models::EnumerationNames::Vorgangstypen,
                    BTreeMap::from_iter(vec![("vorgang", "typ")].drain(..)),
                ),
                (
                    models::EnumerationNames::Schlagworte,
                    BTreeMap::from_iter(
                        vec![
                            ("rel_dok_schlagwort", "sw_id"),
                            ("rel_station_schlagwort", "sw_id"),
                        ]
                        .drain(..),
                    ),
                ),
            ]
            .drain(..),
        );
        for (&table, &column) in enum_table_refs[&path_params.name].iter() {
            sqlx::query(&format!(
                "
            WITH lookup AS (SELECT * FROM UNNEST($1::int4[], $2::int4[]) AS la(new, old))
            UPDATE {table} 
            SET {column} = (SELECT new FROM lookup WHERE old={column})
            WHERE {column} = ANY($2::int4[])"
            ))
            .bind(&rep_new[..])
            .bind(&rep_old[..])
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(&format!(
            "DELETE FROM {} x WHERE x.id = ANY($1::int4[])",
            enum_tables[&path_params.name]
        ))
        .bind(&rep_old[..])
        .execute(&mut *tx)
        .await?;

        // return 201Created
        tx.commit().await?;
        Ok(EnumPutResponse::Status201_Created {
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
        })
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
#[cfg(test)]
mod test_authorisiert {
    use std::str::FromStr;

    use crate::api::auth::APIScope;
    use axum::http::Method;
    use axum_extra::extract::{CookieJar, Host};
    use openapi::apis::data_administration_miscellaneous::{
        AutorenDeleteByParamResponse, AutorenPutResponse, DataAdministrationMiscellaneous,
        EnumDeleteResponse, EnumPutResponse, GremienDeleteByParamResponse, GremienPutResponse,
    };
    use openapi::apis::data_administration_vorgang::DataAdministrationVorgang;
    use openapi::apis::miscellaneous_unauthorisiert::{
        GremienGetResponse, MiscellaneousUnauthorisiert,
    };
    use openapi::models::{self, EnumerationNames};

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
    async fn fetch_all_enumvars(server: &LTZFServer, name: EnumerationNames) -> Vec<String> {
        let entries = server
            .enum_get(
                &Method::GET,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &models::EnumGetPathParams { name },
                &models::EnumGetQueryParams {
                    page: None,
                    per_page: None,
                    contains: None,
                },
            )
            .await
            .unwrap();
        match entries {
            openapi::apis::miscellaneous_unauthorisiert::EnumGetResponse::Status200_Success {
                body,
                ..
            } => body,
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
        insert_default_vorgang(&scenario.server).await;

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

    async fn ap_with(
        server: &LTZFServer,
        apr: &models::AutorenPutRequest,
    ) -> crate::Result<AutorenPutResponse> {
        server
            .autoren_put(
                &Method::PUT,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::KeyAdder, 1),
                apr,
            )
            .await
    }
    #[tokio::test]
    async fn test_autor_put() {
        let scenario = TestSetup::new("test_autor_put").await;
        insert_default_vorgang(&scenario.server).await;

        // check permissions
        let response = scenario
            .server
            .autoren_put(
                &Method::PUT,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::Collector, 1),
                &models::AutorenPutRequest {
                    objects: vec![],
                    replacing: None,
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            response,
            AutorenPutResponse::Status403_Forbidden { .. }
        ));
        let other_autor = models::Autor {
            fachgebiet: Some("Blattzerfetzung".to_string()),
            lobbyregister: Some("https://example.com/einzigartig".to_string()),
            person: Some("Thorbjörn Alman".to_string()),
            organisation: "Schmiedeversammlung Süd".to_string(),
        };
        // check insert without conflict
        let autoren = fetch_all_authors(&scenario.server).await;
        let response = ap_with(
            &scenario.server,
            &models::AutorenPutRequest {
                objects: vec![other_autor.clone()],
                replacing: None,
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            response,
            AutorenPutResponse::Status201_Created { .. }
        ));
        let autoren_new = fetch_all_authors(&scenario.server).await;
        assert!(autoren.len() < autoren_new.len());
        assert!(autoren_new.contains(&other_autor));
        let autoren = autoren_new;

        // check insert with conflict
        let response = ap_with(
            &scenario.server,
            &models::AutorenPutRequest {
                objects: vec![other_autor.clone()],
                replacing: None,
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            response,
            AutorenPutResponse::Status304_NotModified { .. }
        ));
        let autoren_new = fetch_all_authors(&scenario.server).await;
        assert_eq!(autoren.len(), autoren_new.len());
        let autoren = autoren_new;

        // check replace
        let repl_grm = models::Autor {
            fachgebiet: Some("Blattzusammensetzung".to_string()),
            lobbyregister: Some("https://example.com/einzigartig/hahadochnicht".to_string()),
            person: Some("Karla Kolumna".to_string()),
            organisation: "Wasserstoffwirtschaftsverband der Ostgoten".to_string(),
        };
        let response = ap_with(
            &scenario.server,
            &models::AutorenPutRequest {
                objects: vec![repl_grm.clone()],
                replacing: Some(vec![models::AutorenPutRequestReplacingInner {
                    replaced_by: 0,
                    values: vec![other_autor.clone()],
                }]),
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            response,
            AutorenPutResponse::Status201_Created { .. }
        ));
        let gremien_new = fetch_all_authors(&scenario.server).await;
        assert_eq!(autoren.len(), gremien_new.len());
        assert!(gremien_new.contains(&repl_grm));

        // malformed request
        let response = ap_with(
            &scenario.server,
            &models::AutorenPutRequest {
                objects: vec![repl_grm.clone()],
                replacing: Some(vec![models::AutorenPutRequestReplacingInner {
                    replaced_by: 1,
                    values: vec![other_autor.clone()],
                }]),
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            response,
            AutorenPutResponse::Status400_BadRequest { .. }
        ));
        let gremien_new = fetch_all_authors(&scenario.server).await;
        assert_eq!(autoren.len(), gremien_new.len());

        scenario.teardown().await;
    }

    async fn ep_with(
        server: &LTZFServer,
        tp: models::EnumerationNames,
        body: &models::EnumPutRequest,
    ) -> crate::Result<EnumPutResponse> {
        server
            .enum_put(
                &Method::PUT,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::KeyAdder, 1),
                &models::EnumPutPathParams { name: tp },
                body,
            )
            .await
    }

    #[tokio::test]
    async fn test_enum_put() {
        let scenario = TestSetup::new("test_enum_put").await;
        insert_default_vorgang(&scenario.server).await;

        // check permissions
        let response = scenario
            .server
            .enum_put(
                &Method::PUT,
                &Host("localhost".to_string()),
                &CookieJar::new(),
                &(APIScope::Collector, 1),
                &models::EnumPutPathParams {
                    name: models::EnumerationNames::Dokumententypen,
                },
                &models::EnumPutRequest {
                    objects: vec![],
                    replacing: None,
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            response,
            EnumPutResponse::Status403_Forbidden { .. }
        ));
        let testcases = vec![
            (
                models::EnumerationNames::Parlamente,
                "EP".to_string(),
                "ER".to_string(),
            ),
            (
                models::EnumerationNames::Dokumententypen,
                "traktat".to_string(),
                "encyclika".to_string(),
            ),
            (
                models::EnumerationNames::Vorgangstypen,
                "Verdauung".to_string(),
                "Rohrreinigung".to_string(),
            ),
            (
                models::EnumerationNames::Schlagworte,
                "flüssiggasterminal".to_string(),
                "rühreihöchstmenge".to_string(),
            ),
            (
                models::EnumerationNames::Vgidtypen,
                "anschrift".to_string(),
                "hausnummer".to_string(),
            ),
            (
                models::EnumerationNames::Stationstypen,
                "hauptbahnhof".to_string(),
                "haltestelle".to_string(),
            ),
        ];
        for (tp, new_entry, other_new_entry) in testcases.iter() {
            let entries = fetch_all_enumvars(&scenario.server, *tp).await;
            let response = ep_with(
                &scenario.server,
                *tp,
                &models::EnumPutRequest {
                    objects: vec![new_entry.clone()],
                    replacing: None,
                },
            )
            .await
            .unwrap();
            assert!(matches!(
                response,
                EnumPutResponse::Status201_Created { .. }
            ));
            let entries_new = fetch_all_enumvars(&scenario.server, *tp).await;
            assert!(entries.len() < entries_new.len());
            assert!(entries_new.contains(&new_entry));
            let entries = entries_new;

            // with conflict
            let response = ep_with(
                &scenario.server,
                *tp,
                &models::EnumPutRequest {
                    objects: vec![new_entry.clone()],
                    replacing: None,
                },
            )
            .await
            .unwrap();
            assert!(matches!(
                response,
                EnumPutResponse::Status304_NotModified { .. }
            ));
            let entries_new = fetch_all_enumvars(&scenario.server, *tp).await;
            assert_eq!(entries.len(), entries_new.len());
            let entries = entries_new;

            // check replace
            let response = ep_with(
                &scenario.server,
                *tp,
                &models::EnumPutRequest {
                    objects: vec![other_new_entry.clone()],
                    replacing: Some(vec![models::EnumPutRequestReplacingInner {
                        replaced_by: 0,
                        values: vec![new_entry.clone()],
                    }]),
                },
            )
            .await
            .unwrap();
            assert!(matches!(
                response,
                EnumPutResponse::Status201_Created { .. }
            ));
            let entries_new = fetch_all_enumvars(&scenario.server, *tp).await;
            assert_eq!(entries.len(), entries_new.len());
            assert!(entries_new.contains(&other_new_entry));
            assert!(!entries_new.contains(&new_entry));

            // malformed request

            let response = ep_with(
                &scenario.server,
                *tp,
                &models::EnumPutRequest {
                    objects: vec![other_new_entry.clone()],
                    replacing: Some(vec![models::EnumPutRequestReplacingInner {
                        replaced_by: 1,
                        values: vec![new_entry.clone()],
                    }]),
                },
            )
            .await
            .unwrap();
            assert!(matches!(
                response,
                EnumPutResponse::Status400_BadRequest { .. }
            ));
            let entries_new = fetch_all_enumvars(&scenario.server, *tp).await;
            assert_eq!(entries.len(), entries_new.len());
        }

        scenario.teardown().await;
    }
}
