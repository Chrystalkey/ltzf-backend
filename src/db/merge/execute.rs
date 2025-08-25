use super::MatchState;
use crate::db::KeyIndex;
use crate::db::insert::{self, insert_or_retrieve_autor};
use crate::error::DataValidationError;
use crate::utils::notify::notify_ambiguous_match;
/// Handles merging of two datasets.
/// vorgang, station and dokument are mergeable, meaning their data is not atomic.
/// Stellungnahme is handled like dokument with the rest being overridable data points
/// API_ID or other uniquely identifying information is not overridden, but preserved.
/// array-like structures are merged by a modified union operation:
/// for each element:
///     - if it is mergeable and one merge candidate found, merge
///     - if it is not mergeable and has a match in the existing set, the existing element takes precedence and is not replaced
///     - if it is not mergeable and has no match it is added to the set.
use crate::{LTZFServer, Result};
use openapi::models;
use uuid::Uuid;

use super::candidates::*;

/// basic data items are to be overridden by newer information.
/// Excempt from this is the api_id, since this is a permanent document identifier.
/// All
pub async fn execute_merge_dokument(
    model: &models::Dokument,
    candidate: i32,
    scraper_id: Uuid,
    collector_key: KeyIndex,
    tx: &mut sqlx::PgTransaction<'_>,
    srv: &LTZFServer,
) -> Result<()> {
    let db_id = candidate;
    // master update
    sqlx::query!(
        "UPDATE dokument SET
        drucksnr = $2, titel =$3,
        kurztitel = COALESCE($4, kurztitel), vorwort=COALESCE($5, vorwort),
        volltext=COALESCE($6, volltext), zusammenfassung=COALESCE($7, zusammenfassung),
        zp_lastmod=$8, link=$9, hash=$10, meinung=$11
        WHERE dokument.id = $1
        ",
        db_id,
        model.drucksnr,
        model.titel,
        model.kurztitel,
        model.vorwort,
        model.volltext,
        model.zusammenfassung,
        model.zp_modifiziert,
        model.link,
        model.hash,
        model.meinung.map(|x| x as i32)
    )
    .execute(&mut **tx)
    .await?;
    // schlagworte::UNION
    insert::insert_dok_sw(db_id, model.schlagworte.clone().unwrap_or_default(), tx).await?;
    // autoren::UNION
    let mut aids = vec![];
    for a in &model.autoren {
        aids.push(insert_or_retrieve_autor(a, tx, srv).await?);
    }
    sqlx::query!(
        "INSERT INTO rel_dok_autor(dok_id, aut_id)
    SELECT $1, blub FROM UNNEST($2::int4[]) as blub 
    ON CONFLICT DO NOTHING",
        db_id,
        &aids[..]
    )
    .execute(&mut **tx)
    .await?;

    sqlx::query!(
        "INSERT INTO scraper_touched_dokument(dok_id, collector_key, scraper) 
    VALUES ($1, $2, $3) 
    ON CONFLICT(dok_id, scraper) DO UPDATE SET time_stamp=NOW()",
        db_id,
        collector_key,
        scraper_id
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "WITH ranked_objects AS (
        SELECT dok_id, scraper, 
        ROW_NUMBER() OVER (
            PARTITION BY dok_id
            ORDER BY time_stamp DESC
        ) AS rn 
        FROM scraper_touched_dokument
        )
        DELETE FROM scraper_touched_dokument st
        USING ranked_objects ro
        WHERE st.dok_id=ro.dok_id AND
        st.scraper=ro.scraper AND
        ro.rn > $1",
        srv.config.per_object_scraper_log_size as i64
    )
    .execute(&mut **tx)
    .await?;
    tracing::info!("Merging Dokument into Database successful");
    Ok(())
}
pub async fn insert_or_merge_dok(
    dok: &models::StationDokumenteInner,
    scraper_id: Uuid,
    collector_key: KeyIndex,
    tx: &mut sqlx::PgTransaction<'_>,
    srv: &LTZFServer,
) -> Result<Option<i32>> {
    match dok {
        models::StationDokumenteInner::String(uuid) => {
            let uuid = uuid::Uuid::parse_str(uuid)?;
            let id = sqlx::query!("SELECT id FROM dokument d WHERE d.api_id = $1", uuid)
                .map(|r| r.id)
                .fetch_optional(&mut **tx)
                .await?;
            if let Some(id) = id {
                Ok(Some(id))
            } else {
                Err(DataValidationError::IncompleteDataSupplied {
                    input: format!("Supplied uuid `{uuid}` as document id without a body, but no such ID is in the database.") }.into())
            }
        }
        models::StationDokumenteInner::Dokument(dok) => {
            let matches = dokument_merge_candidates(dok, &mut **tx, srv).await?;
            match matches {
                MatchState::NoMatch => {
                    let did = crate::db::insert::insert_dokument(
                        (**dok).clone(),
                        scraper_id,
                        collector_key,
                        tx,
                        srv,
                    )
                    .await?;
                    Ok(Some(did))
                }
                MatchState::ExactlyOne(matchmod) => {
                    tracing::debug!(
                        "Found exactly one match with db id: {}. Merging...",
                        matchmod
                    );
                    execute_merge_dokument(dok, matchmod, scraper_id, collector_key, tx, srv)
                        .await?;
                    Ok(None)
                }
                MatchState::Ambiguous(matches) => {
                    let api_ids = sqlx::query!(
                        "SELECT api_id FROM dokument WHERE id = ANY($1::int4[])",
                        &matches[..]
                    )
                    .map(|r| r.api_id)
                    .fetch_all(&mut **tx)
                    .await?;
                    notify_ambiguous_match(
                        api_ids,
                        &**dok,
                        "execute merge station.dokumente",
                        srv,
                    )?;
                    Err(DataValidationError::AmbiguousMatch {
                        message: "Ambiguous document match(station), see notification".to_string(),
                    }
                    .into())
                }
            }
        }
    }
}

pub async fn execute_merge_station(
    model: &models::Station,
    candidate: i32,
    scraper_id: Uuid,
    collector_key: KeyIndex,
    tx: &mut sqlx::PgTransaction<'_>,
    srv: &LTZFServer,
) -> Result<()> {
    let db_id = candidate;
    let obj = "merge station";
    let sapi = sqlx::query!("SELECT api_id FROM station WHERE id = $1", db_id)
        .map(|x| x.api_id)
        .fetch_one(&mut **tx)
        .await?;
    // pre-master updates
    let gr_id = insert::insert_or_retrieve_gremium(&model.gremium, tx, srv).await?;
    // master update
    sqlx::query!(
        "UPDATE station SET 
        gr_id = COALESCE($2, gr_id),
        typ = (SELECT id FROM stationstyp WHERE value = $3),
        titel = COALESCE($4, titel),
        zp_start = $5, zp_modifiziert = COALESCE($6, NOW()),
        trojanergefahr = COALESCE($7, trojanergefahr),
        link = COALESCE($8, link),
        gremium_isff = $9
        WHERE station.id = $1",
        db_id,
        gr_id,
        srv.guard_ts(model.typ, sapi, obj)?,
        model.titel,
        model.zp_start,
        model.zp_modifiziert,
        model.trojanergefahr.map(|x| x as i32),
        model.link,
        model.gremium_federf
    )
    .execute(&mut **tx)
    .await?;

    // links::UNION
    sqlx::query!(
        "INSERT INTO rel_station_link(stat_id, link)
        SELECT $1, blub FROM UNNEST($2::text[]) as blub
        ON CONFLICT DO NOTHING",
        db_id,
        model.additional_links.as_ref().map(|x| &x[..])
    )
    .execute(&mut **tx)
    .await?;

    // schlagworte::UNION
    insert::insert_station_sw(db_id, model.schlagworte.clone().unwrap_or_default(), tx).await?;

    // dokumente::UNION
    let mut insert_ids = vec![];

    for dok in model.dokumente.iter() {
        // if id & not in database: fail.
        // if id & in database: add to list of associated documents
        // if document: match & integrate or insert.
        if let Some(id) = insert_or_merge_dok(dok, scraper_id, collector_key, tx, srv).await? {
            insert_ids.push(id);
        }
        sqlx::query!(
            "INSERT INTO rel_station_dokument(stat_id, dok_id) 
        SELECT $1, did FROM UNNEST($2::int4[]) as did",
            db_id,
            &insert_ids[..]
        )
        .execute(&mut **tx)
        .await?;
    }

    // stellungnahmen
    let mut insert_ids = vec![];
    for stln in model.stellungnahmen.as_ref().unwrap_or(&vec![]) {
        if let Some(id) = insert_or_merge_dok(stln, scraper_id, collector_key, tx, srv).await? {
            insert_ids.push(id);
        }
        sqlx::query!(
            "INSERT INTO rel_station_stln(stat_id, dok_id) 
            SELECT $1, did FROM UNNEST($2::int4[]) as did",
            db_id,
            &insert_ids[..]
        )
        .execute(&mut **tx)
        .await?;
    }
    sqlx::query!(
        "INSERT INTO scraper_touched_station(stat_id, collector_key, scraper) 
        VALUES ($1, $2, $3) ON CONFLICT(stat_id, scraper) DO UPDATE SET time_stamp=NOW()",
        db_id,
        collector_key,
        scraper_id
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "WITH ranked_objects AS (
        SELECT stat_id, scraper, 
        ROW_NUMBER() OVER (
            PARTITION BY stat_id
            ORDER BY time_stamp DESC
        ) AS rn 
        FROM scraper_touched_station
        )
        DELETE FROM scraper_touched_station st
        USING ranked_objects ro
        WHERE st.stat_id=ro.stat_id AND
        st.scraper=ro.scraper AND
        ro.rn > $1",
        srv.config.per_object_scraper_log_size as i64
    )
    .execute(&mut **tx)
    .await?;
    tracing::info!("Merging Station into Database successful");
    Ok(())
}

pub async fn execute_merge_vorgang(
    model: &models::Vorgang,
    candidate: i32,
    scraper_id: Uuid,
    collector_key: KeyIndex,
    tx: &mut sqlx::PgTransaction<'_>,
    srv: &LTZFServer,
) -> Result<()> {
    let db_id = candidate;
    let obj = "Vorgang";
    let vapi = model.api_id;
    // master insert
    sqlx::query!(
        "UPDATE vorgang SET
        titel = $1, kurztitel = $2,
        verfaend = $3, wahlperiode = $4,
        typ = (SELECT id FROM vorgangstyp WHERE value = $5)
        WHERE vorgang.id = $6",
        model.titel,
        model.kurztitel,
        model.verfassungsaendernd,
        model.wahlperiode as i32,
        srv.guard_ts(model.typ, vapi, obj)?,
        db_id
    )
    .execute(&mut **tx)
    .await?;
    // initiatoren / initpersonen::UNION
    let mut aids = vec![];
    for a in &model.initiatoren {
        aids.push(insert_or_retrieve_autor(a, tx, srv).await?);
    }
    sqlx::query!(
        "INSERT INTO rel_vorgang_init (vg_id, in_id)
        SELECT $1, blub FROM UNNEST($2::int4[]) as blub
        ON CONFLICT DO NOTHING",
        db_id,
        &aids[..]
    )
    .execute(&mut **tx)
    .await?;
    // links
    let links = model.links.clone().unwrap_or_default();
    sqlx::query!(
        "INSERT INTO rel_vorgang_links (vg_id, link)
        SELECT $1, blub FROM UNNEST($2::text[]) as blub
        ON CONFLICT DO NOTHING",
        db_id,
        &links[..]
    )
    .execute(&mut **tx)
    .await?;
    // identifikatoren
    let ident_list = model
        .ids
        .as_ref()
        .map(|x| x.iter().map(|el| el.id.clone()).collect::<Vec<_>>());

    let identt_list = model.ids.as_ref().map(|x| {
        x.iter()
            .map(|el| srv.guard_ts(el.typ, model.api_id, obj).unwrap())
            .collect::<Vec<_>>()
    });

    sqlx::query!(
        "INSERT INTO rel_vorgang_ident (vg_id, typ, identifikator)
        SELECT $1, vit.id, ident FROM 
        UNNEST($2::text[], $3::text[]) blub(typ_value, ident)
        INNER JOIN vg_ident_typ vit ON vit.value = typ_value
        ON CONFLICT DO NOTHING
        ",
        db_id,
        identt_list.as_ref().map(|x| &x[..]),
        ident_list.as_ref().map(|x| &x[..])
    )
    .execute(&mut **tx)
    .await?;

    for stat in &model.stationen {
        match station_merge_candidates(stat, db_id, &mut **tx, srv).await? {
            MatchState::NoMatch => {
                insert::insert_station(stat.clone(), db_id, scraper_id, collector_key, tx, srv)
                    .await?;
            }
            MatchState::ExactlyOne(_) => {
                // can be ignored bc same as db_id
                execute_merge_station(stat, db_id, scraper_id, collector_key, tx, srv).await?
            }
            MatchState::Ambiguous(matches) => {
                let mids = sqlx::query!(
                    "SELECT api_id FROM station WHERE id = ANY($1::int4[]);",
                    &matches[..]
                )
                .map(|r| r.api_id)
                .fetch_all(&mut **tx)
                .await?;
                notify_ambiguous_match(mids, stat, "exec_merge_vorgang: station matching", srv)?;
            }
        }
    }
    // lobbyregistereinträge are just replaced as-is, no merging
    sqlx::query!("DELETE FROM lobbyregistereintrag WHERE vg_id = $1", db_id)
        .execute(&mut **tx)
        .await?;

    if let Some(lobbyr) = &model.lobbyregister {
        for l in lobbyr {
            let aid = insert_or_retrieve_autor(&l.organisation, tx, srv).await?;
            let lrid = sqlx::query!(
                "INSERT INTO lobbyregistereintrag(intention, interne_id, organisation, vg_id, link)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id",
                &l.intention,
                &l.interne_id,
                &aid,
                db_id,
                &l.link
            )
            .map(|r| r.id)
            .fetch_one(&mut **tx)
            .await?;
            sqlx::query!(
                "INSERT INTO rel_lobbyreg_drucksnr(drucksnr, lob_id) 
            SELECT x, $1 FROM UNNEST($2::text[]) as x(x)",
                lrid,
                &l.betroffene_drucksachen
            )
            .execute(&mut **tx)
            .await?;
        }
    }

    sqlx::query!(
        "INSERT INTO scraper_touched_vorgang(vg_id, collector_key, scraper) VALUES ($1, $2, $3) ON CONFLICT(vg_id, scraper) DO UPDATE SET time_stamp=NOW()",
        db_id,
        collector_key,
        scraper_id
    )
    .execute(&mut **tx)
    .await?;

    sqlx::query!(
        "WITH ranked_objects AS (
        SELECT vg_id, scraper, 
        ROW_NUMBER() OVER (
            PARTITION BY vg_id
            ORDER BY time_stamp DESC
        ) AS rn 
        FROM scraper_touched_vorgang
        )
        DELETE FROM scraper_touched_vorgang stv
        USING ranked_objects ro
        WHERE stv.vg_id=ro.vg_id AND
        stv.scraper=ro.scraper AND
        ro.rn > $1",
        srv.config.per_object_scraper_log_size as i64
    )
    .execute(&mut **tx)
    .await?;

    tracing::info!(
        "Merging of Vg Successful: Merged `{}`(ext) with  `{}`(db)",
        model.api_id,
        sqlx::query!("SELECT api_id FROM vorgang WHERE id = $1", candidate)
            .map(|r| r.api_id)
            .fetch_one(&mut **tx)
            .await?
    );
    Ok(())
}

pub async fn run_integration(
    model: &models::Vorgang,
    scraper_id: Uuid,
    collector_key: KeyIndex,
    server: &LTZFServer,
) -> Result<()> {
    let mut tx = server.sqlx_db.begin().await?;
    tracing::debug!(
        "Looking for Merge Candidates for Vorgang with api_id: {:?}",
        model.api_id
    );
    let candidates = vorgang_merge_candidates(model, &mut *tx, server).await?;
    match candidates {
        MatchState::NoMatch => {
            tracing::info!(
                "No Merge Candidate found, Inserting Complete Vorgang with api_id: {:?}",
                model.api_id
            );
            let model = model.clone();
            insert::insert_vorgang(&model, scraper_id, collector_key, &mut tx, server).await?;
        }
        MatchState::ExactlyOne(one) => {
            let api_id = sqlx::query!("SELECT api_id FROM vorgang WHERE id = $1", one)
                .map(|r| r.api_id)
                .fetch_one(&mut *tx)
                .await?;
            tracing::info!(
                "Matching Vorgang in the DB has api_id: {}, Updating with data from: {}",
                api_id,
                model.api_id
            );
            let model = model.clone();
            execute_merge_vorgang(&model, one, scraper_id, collector_key, &mut tx, server).await?;
        }
        MatchState::Ambiguous(many) => {
            tracing::warn!(
                "Ambiguous matches for Vorgang with api_id: {:?}",
                model.api_id
            );
            tracing::warn!("Transaction not committed, administrators notified");
            tracing::debug!("Details:  {:?} \n\n {:?}", model, many);
            let api_ids = sqlx::query!(
                "SELECT api_id FROM vorgang WHERE id=ANY($1::int4[])",
                &many[..]
            )
            .map(|r| r.api_id)
            .fetch_all(&mut *tx)
            .await?;
            notify_ambiguous_match(api_ids, model, "merging vorgang", server)?;
            tx.rollback().await?;
            return Err(DataValidationError::AmbiguousMatch {
                message: format!(
                    "Tried to merge object with id `{}`, found {} matching VGs.",
                    model.api_id,
                    many.len()
                ),
            }
            .into());
        }
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod scenariotest {
    use crate::utils::test::generate;
    use crate::{
        LTZFServer, Result,
        api::{
            PaginationResponsePart,
            compare::{compare_vorgang, oicomp},
        },
        db::retrieve,
    };
    use openapi::models::{self, StationDokumenteInner};
    use std::str::FromStr;
    use uuid::Uuid;

    struct Scenario {
        context: Vec<models::Vorgang>,
        object: models::Vorgang,
        expected: Vec<models::Vorgang>,
        shouldfail: bool,
        name: &'static str,
    }
    impl Scenario {
        async fn run(&self) -> Result<()> {
            let server = self.setup().await?;
            self.build_context(&server).await?;
            self.place_object(&server).await?;
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            self.check_result(&server).await?;
            self.teardown().await?;
            Ok(())
        }
        async fn setup(&self) -> Result<LTZFServer> {
            generate::setup_server(self.name).await
        }

        async fn teardown(&self) -> Result<()> {
            let dburl = std::env::var("DATABASE_URL")
                .expect("Expected to find working DATABASE_URL for testing");
            let config = crate::Configuration {
                mail_server: None,
                mail_user: None,
                mail_password: None,
                mail_sender: None,
                mail_recipient: None,
                per_object_scraper_log_size: 200,
                req_limit_count: 4096,
                req_limit_interval: 2,
                host: "localhost".to_string(),
                port: 80,
                db_url: dburl.clone(),
                config: None,
                keyadder_key: "tegernsee-apfelsaft-co2grenzwert".to_string(),
                merge_title_similarity: 0.8,
            };
            let master_server = LTZFServer {
                config: config.clone(),
                mailbundle: None,
                sqlx_db: sqlx::postgres::PgPool::connect(&dburl).await?,
            };
            let dropquery = format!(
                "DROP DATABASE IF EXISTS \"testing_{}\" WITH (FORCE);",
                self.name
            );
            sqlx::query(&dropquery)
                .execute(&master_server.sqlx_db)
                .await?;
            Ok(())
        }

        async fn build_context(&self, server: &LTZFServer) -> Result<()> {
            for obj in self.context.iter() {
                super::run_integration(obj, Uuid::nil(), 1, server).await?;
            }
            Ok(())
        }
        async fn place_object(&self, server: &LTZFServer) -> Result<()> {
            super::run_integration(&self.object, Uuid::nil(), 1, server).await?;
            Ok(())
        }
        async fn check_result(&self, server: &LTZFServer) -> Result<()> {
            let paramock = retrieve::VGGetParameters {
                vgtyp: None,
                wp: None,
                inipsn: None,
                iniorg: None,
                inifch: None,
                parlament: None,
                lower_date: None,
                upper_date: None,
            };
            let mut tx = server.sqlx_db.begin().await.unwrap();
            let db_vorgangs = retrieve::vorgang_by_parameter(
                paramock,
                None,
                Some(PaginationResponsePart::MAX_PER_PAGE),
                &mut tx,
            )
            .await
            .unwrap();
            tx.commit().await?;

            let equality = oicomp(&self.expected, &db_vorgangs.1, &compare_vorgang);
            if !equality && !self.shouldfail {
                let exp_content = self
                    .expected
                    .iter()
                    .map(|x| serde_json::to_string_pretty(x).unwrap())
                    .collect::<Vec<_>>()
                    .join(",");
                let dbv_content = db_vorgangs
                    .1
                    .iter()
                    .map(|x| serde_json::to_string_pretty(x).unwrap())
                    .collect::<Vec<_>>()
                    .join(",");
                std::fs::write(
                    format!("tests/{}_dump.json", self.name),
                    format!("{{\n\"expected\": [{exp_content}],\n\"actual\": [{dbv_content}]}}"),
                )
                .unwrap();
                assert!(
                    false,
                    "Expected and Actual Contents were not equal. Dump: tests/{}_dump.json",
                    self.name
                );
            }
            assert!(
                !(equality && self.shouldfail),
                "Expected Case to fail, but actual output was equal to expectation"
            );
            Ok(())
        }
    }
    fn vg_to_expected(vg: &models::Vorgang) -> models::Vorgang {
        let mut vg = vg.clone();
        for s in &mut vg.stationen {
            for d in &mut s.dokumente {
                if let StationDokumenteInner::Dokument(dok) = d {
                    *d = StationDokumenteInner::String(Box::new(dok.api_id.unwrap().to_string()));
                }
            }
            if s.stellungnahmen.is_none() {
                continue;
            }
            for d in s.stellungnahmen.as_mut().unwrap() {
                if let StationDokumenteInner::Dokument(dok) = d {
                    *d = StationDokumenteInner::String(Box::new(dok.api_id.unwrap().to_string()));
                }
            }
        }
        return vg;
    }
    // one in, again one in, one out
    #[tokio::test]
    async fn test_idempotenz() {
        let vg = generate::default_vorgang();
        let scenario = Scenario {
            context: vec![vg.clone()],
            object: vg.clone(),
            expected: vec![vg_to_expected(&vg)],
            name: "idempotenz",
            shouldfail: false,
        };
        scenario.run().await.unwrap();
    }
    #[tokio::test]
    async fn test_merge_matching_ids() {
        let vg = generate::default_vorgang();
        let mut vg2 = generate::default_vorgang();
        vg2.api_id = Uuid::nil(); // take out api id matching
        vg2.titel = "Anderer Titel".to_string();
        vg2.stationen = vec![generate::alternate_station()]; // take out vorwort matching

        let mut vg_exp = vg.clone();
        vg_exp.titel = vg2.titel.clone();
        vg_exp.stationen = vec![generate::default_station(), generate::alternate_station()];

        let scenario = Scenario {
            name: "merge_matching_ids",
            shouldfail: false,
            context: vec![vg],
            object: vg2,
            expected: vec![vg_to_expected(&vg_exp)],
        };
        scenario.run().await.unwrap();
    }
    #[tokio::test]
    async fn test_link_ini_ids_merging() {
        let vg = generate::default_vorgang();
        let mut vg_mod = vg.clone();
        vg_mod.links = Some(vec!["https://example.com".to_string()]);
        vg_mod.ids = Some(vec![models::VgIdent {
            id: "einzigartig und anders".to_string(),
            typ: models::VgIdentTyp::Initdrucks,
        }]);
        vg_mod.initiatoren = vec![models::Autor {
            person: Some("Max Mustermann".to_string()),
            organisation: "Musterorganisation".to_string(),
            fachgebiet: Some("Musterfachgebiet".to_string()),
            lobbyregister: Some("Musterlobbyregister".to_string()),
        }];

        let mut vg_exp = vg.clone();
        vg_exp.links = Some(
            [].iter()
                .chain(vg.links.as_ref().unwrap().iter())
                .chain(vg_mod.links.as_ref().unwrap().iter())
                .cloned()
                .collect(),
        ); //merged
        vg_exp.links.as_mut().unwrap().sort();
        vg_exp.ids = Some(
            [].iter()
                .chain(vg.ids.as_ref().unwrap().iter())
                .chain(vg_mod.ids.as_ref().unwrap().iter())
                .cloned()
                .collect(),
        ); //merged
        vg_exp.ids.as_mut().unwrap().sort_by(|a, b| a.id.cmp(&b.id));
        vg_exp.initiatoren = []
            .iter()
            .chain(vg.initiatoren.iter())
            .chain(vg_mod.initiatoren.iter())
            .cloned()
            .collect(); //merged
        vg_exp
            .initiatoren
            .sort_by(|a, b| a.organisation.cmp(&b.organisation));
        let scenario = Scenario {
            context: vec![vg],
            object: vg_mod,
            expected: vec![vg_to_expected(&vg_exp)],
            name: "link_ini_ids_merging",
            shouldfail: false,
        };
        scenario.run().await.unwrap();
    }
    #[tokio::test]
    async fn test_vorgang_weak_property_change_override() {
        let vg = generate::default_vorgang();
        let mut vg_mod = vg.clone();
        vg_mod.titel = "Testtitel".to_string();
        vg_mod.kurztitel = Some("Testkurztitel".to_string());
        vg_mod.wahlperiode = 20;
        vg_mod.verfassungsaendernd = true;
        let scenario = Scenario {
            context: vec![vg.clone()],
            object: vg_mod.clone(),
            expected: vec![vg_to_expected(&vg_mod)],
            name: "weak_prop_change_override",
            shouldfail: false,
        };
        scenario.run().await.unwrap();
    }
    #[tokio::test]
    async fn test_not_merged_but_separate() {
        let vg = generate::default_vorgang();
        let mut vg2 = vg.clone();
        vg2.api_id = Uuid::from_str("b18bee64-c0ff-eeee-ff1c-deadbeef3452").unwrap();
        vg2.ids = None;

        let mut stat = generate::default_station();
        stat.api_id = Some(Uuid::from_str("b18bee64-c0ff-eeee-ff1c-deadbeef4732").unwrap());
        stat.typ = models::Stationstyp::PostparlGsblt;
        stat.dokumente = vec![];
        vg2.stationen = vec![stat];

        vg2.titel = "Ich Mag Moneten und deshalb ist das ein anderes Gesetz".to_string();
        let scenario = Scenario {
            context: vec![vg.clone()],
            object: vg2.clone(),
            expected: vec![vg_to_expected(&vg), vg_to_expected(&vg2)],
            name: "not_merged_but_separate",
            shouldfail: false,
        };
        scenario.run().await.unwrap();
    }
    #[tokio::test]
    async fn test_schlagwort_duplicate_elimination_and_formatting() {
        let mut vg = generate::default_vorgang();
        vg.stationen[0].schlagworte = Some(vec![
            "AiNz".to_string(),
            "ainz".to_string(),
            "AINZ".to_string(),
        ]);
        let vg2 = vg.clone();

        let mut vg_exp = vg.clone();
        vg_exp.stationen[0].schlagworte = Some(vec!["ainz".to_string()]);
        let scenario = Scenario {
            context: vec![vg],
            object: vg2,
            expected: vec![vg_to_expected(&vg_exp)],
            shouldfail: false,
            name: "schlagwort_duplicate_elimination_and_formatting",
        };
        scenario.run().await.unwrap();
    }
    #[tokio::test]
    async fn test_station_merging_on_weak_property_changes() {
        let vg = generate::default_vorgang();
        let mut vg2 = vg.clone();
        vg2.stationen[0].link = Some("https://other.link".to_string());
        vg2.stationen[0].titel = Some("Weirder anderer Titel".to_string());
        vg2.stationen[0].zp_modifiziert = Some(chrono::Utc::now());
        vg2.stationen[0].gremium_federf = Some(true);
        vg2.stationen[0].trojanergefahr = Some(4u8);
        vg2.stationen[0].zp_start = chrono::Utc::now();

        let scenario = Scenario {
            context: vec![vg],
            object: vg2.clone(),
            expected: vec![vg_to_expected(&vg2)],
            name: "station_weak_props_change",
            shouldfail: false,
        };
        scenario.run().await.unwrap();
    }
    #[tokio::test]
    async fn test_dokument_merging_on_weak_property_changes() {
        let modified_dokument = models::Dokument {
            api_id: Some(Uuid::from_str("b18bee64-c0ff-ff0c-ff1c-deadbeef4732").unwrap()),
            titel: "Anderer Titel".to_string(),
            ..generate::default_dokument()
        };
        let modified_stellungnahme = models::Dokument {
            api_id: Some(Uuid::from_str("b18bee64-c0ff-ff1c-ff1c-deadbeef4732").unwrap()),
            titel: "Anderer Titel für ne Stellungnahme".to_string(),
            ..generate::default_stellungnahme()
        };
        let modified_docs_vorgang = models::Vorgang {
            stationen: vec![models::Station {
                dokumente: vec![models::StationDokumenteInner::Dokument(Box::new(
                    modified_dokument.clone(),
                ))],
                stellungnahmen: Some(vec![models::StationDokumenteInner::Dokument(Box::new(
                    modified_stellungnahme.clone(),
                ))]),
                ..generate::default_station()
            }],
            ..generate::default_vorgang()
        };
        let expected_vorgang = models::Vorgang {
            stationen: vec![models::Station {
                dokumente: vec![models::StationDokumenteInner::Dokument(Box::new(
                    models::Dokument {
                        api_id: generate::default_dokument().api_id,
                        ..modified_dokument.clone()
                    },
                ))],
                stellungnahmen: Some(vec![models::StationDokumenteInner::Dokument(Box::new(
                    models::Dokument {
                        api_id: generate::default_stellungnahme().api_id,
                        ..modified_stellungnahme.clone()
                    },
                ))]),
                ..generate::default_station()
            }],
            ..generate::default_vorgang()
        };
        let scenario = Scenario {
            context: vec![generate::default_vorgang()],
            object: modified_docs_vorgang,
            expected: vec![vg_to_expected(&expected_vorgang)],
            name: "dokument_merging_on_weak_property_changes",
            shouldfail: false,
        };
        scenario.run().await.unwrap();
    }
}
