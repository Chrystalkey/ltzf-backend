#![allow(unused)]
use super::MatchState;
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

/// this function determines what means "matching enough".
/// 1. wenn api_id matcht
/// 2. wenn wp, typ und mindestens ein identifikator matchen
/// 3. wenn wp, typ und das vorwort sich "sehr ähnlich sind (>0.8)"
pub async fn vorgang_merge_candidates(
    model: &models::Vorgang,
    executor: impl sqlx::PgExecutor<'_>,
    srv: &LTZFServer,
) -> Result<MatchState<i32>> {
    let obj = "merged Vorgang";
    let ident_t: Vec<_> = model
        .ids
        .as_ref()
        .unwrap_or(&vec![])
        .iter()
        .map(|x| x.id.clone())
        .collect();
    let identt_t: Vec<_> = model
        .ids
        .as_ref()
        .unwrap_or(&vec![])
        .iter()
        .map(|x| srv.guard_ts(x.typ, model.api_id, obj).unwrap())
        .collect();
    let initds: Vec<_> = model
        .stationen
        .iter()
        .filter(|&s| s.typ == models::Stationstyp::ParlInitiativ)
        .flat_map(|s| {
            s.dokumente
                .iter()
                .filter(|&d| {
                    if let models::StationDokumenteInner::Dokument(d) = d {
                        d.typ == models::Doktyp::Entwurf && d.vorwort.is_some()
                    } else {
                        false
                    }
                })
                .map(|d| {
                    if let models::StationDokumenteInner::Dokument(d) = d {
                        d.vorwort.clone().unwrap()
                    } else {
                        unreachable!()
                    }
                })
                .map(|s| s.to_string())
        })
        .collect();
    let result = sqlx::query!(
        "WITH db_id_table AS (
            SELECT rel_vorgang_ident.vg_id as vg_id, identifikator as ident, vg_ident_typ.value as idt_str
            FROM vg_ident_typ, rel_vorgang_ident 
            WHERE vg_ident_typ.id = rel_vorgang_ident.typ),
	initds_vwtable AS ( --vorworte von initiativdrucksachen von stationen
			SELECT s.vg_id, d.vorwort, d.volltext FROM dokument d
				INNER JOIN rel_station_dokument rsd ON rsd.dok_id=d.id
				INNER JOIN dokumententyp dt ON dt.id=d.typ
				INNER JOIN station s ON s.id = rsd.stat_id
				WHERE rsd.stat_id=s.id
				AND (dt.value='entwurf' OR dt.value = 'preparl-entwurf')
		)

SELECT DISTINCT(vorgang.id), vorgang.api_id FROM vorgang -- gib vorgänge, bei denen
	INNER JOIN vorgangstyp vt ON vt.id = vorgang.typ
	WHERE
	vorgang.api_id = $1 OR -- entweder die API ID genau übereinstimmt (trivialer Fall) ODER
	(
	vorgang.wahlperiode = $4 AND -- wahlperiode und 
	vt.value = $5 AND            -- typ übereinstimmen und 
		(EXISTS (SELECT * FROM UNNEST($2::text[], $3::text[]) as eingabe(ident, typ), db_id_table WHERE  -- eine übereinstimmende ID existiert
			db_id_table.vg_id = vorgang.id AND
			eingabe.ident = db_id_table.ident AND
			eingabe.typ = db_id_table.idt_str)
		OR -- oder 
		EXISTS (SELECT * FROM UNNEST($6::text[]) eingabe(vw), initds_vwtable ids
		WHERE ids.vg_id = vorgang.id
		AND SIMILARITY(vw, ids.vorwort) > 0.8
		)
		)
	);",
    model.api_id, &ident_t[..], &identt_t[..], model.wahlperiode as i32,
    srv.guard_ts(model.typ, model.api_id, obj)?, &initds[..])
    .fetch_all(executor).await?;

    tracing::debug!(
        "Found {} matches for Vorgang with api_id: {}",
        result.len(),
        model.api_id
    );

    Ok(match result.len() {
        0 => MatchState::NoMatch,
        1 => MatchState::ExactlyOne(result[0].id),
        _ => {
            tracing::warn!(
                "Mehrere Vorgänge gefunden, die als Kandidaten für Merge infrage kommen für den Vorgang `{}`:\n{:?}",
                model.api_id,
                result.iter().map(|r| r.api_id).collect::<Vec<_>>()
            );
            MatchState::Ambiguous(result.iter().map(|x| x.id).collect())
        }
    })
}

/// bei gleichem Vorgang => Vorraussetzung
/// 1. wenn die api_id matcht
/// 2. wenn typ, parlament, gremium matcht und mindestens ein Dokument gleich ist
pub async fn station_merge_candidates(
    model: &models::Station,
    vorgang: i32,
    executor: impl sqlx::PgExecutor<'_>,
    srv: &LTZFServer,
) -> Result<MatchState<i32>> {
    let obj = "merged station";
    let api_id = model.api_id.unwrap_or(uuid::Uuid::now_v7());
    let dok_hash: Vec<_> = model
        .dokumente
        .iter()
        .filter(|x| matches!(x, models::StationDokumenteInner::Dokument(_)))
        .map(|x| {
            if let models::StationDokumenteInner::Dokument(d) = x {
                d.hash.clone()
            } else {
                unreachable!()
            }
        })
        .collect();
    let (gr_name, gr_wp, gr_parl) = if let Some(gremium) = &model.gremium {
        (
            Some(gremium.name.clone()),
            Some(gremium.wahlperiode as i32),
            Some(gremium.parlament.to_string()),
        )
    } else {
        (None, None, None)
    };
    let result = sqlx::query!(
        "SELECT s.id, s.api_id FROM station s
    INNER JOIN stationstyp st ON st.id=s.typ
    LEFT JOIN gremium g ON g.id=s.gr_id
    LEFT JOIN parlament p ON p.id = g.parl
    WHERE s.api_id = $1 OR
    (s.vg_id = $2 AND st.value = $3 AND  -- vorgang und stationstyp übereinstimmen
    (g.name = $4 OR $4 IS NULL) AND  -- gremiumname übereinstimmt
    (p.value = $5 OR $5 IS NULL) AND  -- parlamentname übereinstimmt
    (g.wp = $6 OR $6 IS NULL) AND -- gremium wahlperiode übereinstimmt
    EXISTS (SELECT * FROM rel_station_dokument rsd
        INNER JOIN dokument d ON rsd.dok_id=d.id
        WHERE rsd.stat_id = s.id
        AND d.hash IN (SELECT str FROM UNNEST($7::text[]) blub(str))
	))",
        model.api_id,
        vorgang,
        srv.guard_ts(model.typ, api_id, obj)?,
        gr_name,
        gr_parl,
        gr_wp,
        &dok_hash[..]
    )
    .fetch_all(executor)
    .await?;
    tracing::debug!(
        "Found {} matches for Station with api_id: {}",
        result.len(),
        api_id
    );

    Ok(match result.len() {
        0 => MatchState::NoMatch,
        1 => MatchState::ExactlyOne(result[0].id),
        _ => {
            tracing::warn!(
                "Mehrere Stationen gefunden, die als Kandidaten für Merge infrage kommen für Station `{}`:\n{:?}",
                api_id,
                result.iter().map(|r| r.api_id).collect::<Vec<_>>()
            );
            MatchState::Ambiguous(result.iter().map(|x| x.id).collect())
        }
    })
}
/// bei gleichem
/// - hash oder api_id oder drucksnr
pub async fn dokument_merge_candidates(
    model: &models::Dokument,
    executor: impl sqlx::PgExecutor<'_>,
    srv: &LTZFServer,
) -> Result<MatchState<i32>> {
    let dids = sqlx::query!(
        "SELECT d.id FROM dokument d 
        INNER JOIN dokumententyp dt ON dt.id = d.typ 
        WHERE 
        (d.hash = $1 OR
        d.api_id = $2 OR
        d.drucksnr = $3) AND dt.value = $4",
        model.hash,
        model.api_id,
        model.drucksnr,
        srv.guard_ts(
            model.typ,
            model.api_id.unwrap_or(Uuid::nil()),
            "dok_merge_candidates"
        )?
    )
    .map(|r| r.id)
    .fetch_all(executor)
    .await?;
    if dids.is_empty() {
        Ok(MatchState::NoMatch)
    } else if dids.len() == 1 {
        Ok(MatchState::ExactlyOne(dids[0]))
    } else {
        Ok(MatchState::Ambiguous(dids))
    }
}

/// basic data items are to be overridden by newer information.
/// Excempt from this is the api_id, since this is a permanent document identifier.
/// All
pub async fn execute_merge_dokument(
    model: &models::Dokument,
    candidate: i32,
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

    tracing::info!("Merging Dokument into Database successful");
    Ok(())
}

pub async fn execute_merge_station(
    model: &models::Station,
    candidate: i32,
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
    let gr_id = if let Some(gremium) = &model.gremium {
        let id = insert::insert_or_retrieve_gremium(gremium, tx, srv).await?;
        Some(id)
    } else {
        None
    };
    // master update
    sqlx::query!(
        "UPDATE station SET 
        gr_id = COALESCE($2, gr_id),
        p_id = (SELECT id FROM parlament WHERE value = $3),
        typ = (SELECT id FROM stationstyp WHERE value = $4),
        titel = COALESCE($5, titel),
        zp_start = $6, zp_modifiziert = COALESCE($7, NOW()),
        trojanergefahr = COALESCE($8, trojanergefahr),
        link = COALESCE($9, link),
        gremium_isff = $10
        WHERE station.id = $1",
        db_id,
        gr_id,
        model.parlament.to_string(),
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
        match dok {
            models::StationDokumenteInner::String(uuid) => {
                let uuid = uuid::Uuid::parse_str(&uuid)?;
                let id = sqlx::query!("SELECT id FROM dokument d WHERE d.api_id = $1", uuid)
                    .map(|r| r.id)
                    .fetch_optional(&mut **tx)
                    .await?;
                if id.is_none() {
                    return Err(DataValidationError::IncompleteDataSupplied {
                        input: format!("Supplied uuid `{}` as document id for station `{}`, but no such ID is in the database.",
                        uuid, sapi) }.into());
                }
                insert_ids.push(id.unwrap());
            }
            models::StationDokumenteInner::Dokument(dok) => {
                let matches = dokument_merge_candidates(dok, &mut **tx, srv).await?;
                match matches {
                    MatchState::NoMatch => {
                        let did =
                            crate::db::insert::insert_dokument((**dok).clone(), tx, srv).await?;
                        insert_ids.push(did);
                    }
                    MatchState::ExactlyOne(matchmod) => {
                        tracing::debug!(
                            "Found exactly one match with db id: {}. Merging...",
                            matchmod
                        );
                        execute_merge_dokument(dok, matchmod, tx, srv).await?;
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
                        return Err(DataValidationError::AmbiguousMatch {
                            message: "Ambiguous document match(station), see notification"
                                .to_string(),
                        }
                        .into());
                    }
                }
            }
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
    for stln in model.stellungnahmen.as_ref().unwrap_or(&vec![]) {
        match dokument_merge_candidates(stln, &mut **tx, srv).await? {
            MatchState::NoMatch => {
                let did = insert::insert_dokument(stln.clone(), tx, srv).await?;
                sqlx::query!(
                    "INSERT INTO rel_station_stln(stat_id, dok_id) VALUES($1, $2) ON CONFLICT DO NOTHING;",
                    db_id,
                    did
                )
                .execute(&mut **tx)
                .await?;
            }
            MatchState::ExactlyOne(did) => {
                execute_merge_dokument(stln, did, tx, srv).await?;
            }
            MatchState::Ambiguous(matches) => {
                let api_ids = sqlx::query!(
                    "SELECT api_id FROM dokument WHERE id = ANY($1::int4[])",
                    &matches[..]
                )
                .map(|r| r.api_id)
                .fetch_all(&mut **tx)
                .await?;
                notify_ambiguous_match(api_ids, stln, "execute merge station.stellungnahmen", srv)?;
                return Err(DataValidationError::AmbiguousMatch {
                    message: "Ambiguous document match(Stln), see notification".to_string(),
                }
                .into());
            }
        };
    }
    tracing::info!("Merging Station into Database successful");
    Ok(())
}

pub async fn execute_merge_vorgang(
    model: &models::Vorgang,
    candidate: i32,
    scraper_id: Uuid,
    tx: &mut sqlx::PgTransaction<'_>,
    srv: &LTZFServer,
) -> Result<()> {
    let db_id = candidate;
    let obj = "Vorgang";
    let vapi = model.api_id;
    /// master insert
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
    /// initiatoren / initpersonen::UNION
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
    /// links
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
    /// identifikatoren
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
                insert::insert_station(stat.clone(), db_id, scraper_id, tx, srv).await?;
            }
            MatchState::ExactlyOne(merge_station) => {
                execute_merge_station(stat, db_id, tx, srv).await?
            }
            MatchState::Ambiguous(matches) => {
                let mids = sqlx::query!(
                    "SELECT api_id FROM station WHERE id = ANY($1::int4[]);",
                    &matches[..]
                )
                .map(|r| r.api_id)
                .fetch_all(&mut **tx)
                .await?;
                notify_ambiguous_match(mids, stat, "exec_merge_vorgang: station matching", srv);
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
            insert::insert_vorgang(&model, scraper_id, &mut tx, server).await?;
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
            execute_merge_vorgang(&model, one, scraper_id, &mut tx, server).await?;
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
    use crate::{LTZFError, LTZFServer, Result, api::PaginationResponsePart, db::retrieve};
    use openapi::models;
    use similar::ChangeTag;
    use std::collections::HashSet;
    use uuid::Uuid;

    mod generate {
        use std::str::FromStr;

        use openapi::models;
        use uuid::Uuid;
        pub(crate) fn default_vorgang() -> models::Vorgang {
            let mut at = vec![default_autor_person(), default_autor_institution()];
            at.sort_by(|a, b| a.organisation.cmp(&b.organisation));
            models::Vorgang {
                api_id: Uuid::from_str("b18bde64-c0ff-eeee-ff0c-deadbeef106e").unwrap(),
                titel: "Testtitel".to_string(),
                kurztitel: Some("Kurzer Testtitel".to_string()),
                stationen: vec![default_station()],
                typ: models::Vorgangstyp::GgZustimmung,
                verfassungsaendernd: false,
                wahlperiode: 20,
                touched_by: None,
                links: Some(vec!["https://example.com/ichmagmoneten".to_string()]),
                initiatoren: at,
                ids: Some(vec![models::VgIdent {
                    id: "einzigartig".to_string(),
                    typ: models::VgIdentTyp::Initdrucks,
                }]),
                lobbyregister: Some(vec![models::Lobbyregeintrag {
                    betroffene_drucksachen: vec!["20/2014".to_string()],
                    intention: "Für die Klicks".to_string(),
                    interne_id: "as9d8fja9s8djf".to_string(),
                    link: "https://example.com/einig/gerecht/frei".to_string(),
                    organisation: default_autor_lobby(),
                }]),
            }
        }
        pub(crate) fn default_station() -> models::Station {
            models::Station {
                api_id: Some(Uuid::from_str("b18bde64-c0ff-eeee-ff0c-deadbeefeeee").unwrap()),
                typ: models::Stationstyp::ParlAusschber,
                link: Some("https://an.example.com/leckmichfett".to_string()),
                gremium_federf: Some(false),
                titel: Some("rattlesnakes!".to_string()),
                zp_start: chrono::DateTime::parse_from_rfc3339("1950-01-01T22:01:02+00:00")
                    .unwrap()
                    .to_utc(),
                zp_modifiziert: Some(
                    chrono::DateTime::parse_from_rfc3339("1950-01-01T22:01:02+00:00")
                        .unwrap()
                        .to_utc(),
                ),
                trojanergefahr: Some(2u8),
                parlament: models::Parlament::Bb,
                schlagworte: Some(vec!["stationär".to_string()]),
                touched_by: None,
                stellungnahmen: Some(vec![default_stellungnahme()]),
                additional_links: Some(vec![
                    "https://example.com/videos/aus/der/hoelle".to_string(),
                ]),
                dokumente: vec![models::StationDokumenteInner::Dokument(Box::new(
                    default_dokument(),
                ))],
                gremium: Some(default_gremium()),
            }
        }
        pub(crate) fn default_gremium() -> models::Gremium {
            models::Gremium {
                link: Some("https://a.xyz".to_string()),
                name: "Ausschuss für Inneres und Gemüsaufläufe".to_string(),
                parlament: models::Parlament::Bb,
                wahlperiode: 20,
            }
        }
        pub(crate) fn default_dokument() -> models::Dokument {
            models::Dokument{
                api_id: Some(Uuid::from_str("b18bde64-c0ff-eeee-ff0c-deadbeef3333").unwrap()),
                autoren: vec![default_autor_person()],
                hash: "f98d9d6f136109780d69f6".to_string(),
                drucksnr: Some("20/441".to_string()),
                kurztitel: Some("Dokumentblubgedöns".to_string()),
                link: "https://irgendwo.im.nirgendwo.de".to_string(),
                meinung: None,
                titel: "Ganz ausführlicher Titel, der die Schuppenfärbungsverordnung von 2027 zu verändern versucht bevor sie Gesetz wird".to_string(),
                typ: models::Doktyp::Entwurf,
                volltext: "Nee, ich denk mir hier keinen Volltext aus. Das wär wirklich viel zu lang. Vor allem zu einer Schuppenfärbeverordnung aus der Zukunft! Soo lächerlich. 
                Natürlich mal wieder Klassiker, dass die hier \"Schuppen\" und nicht \"Fischschuppen\", \"Gartenschuppen\" oder \"Drachenschuppen\" geschrieben haben. Danke Merkel! 
                Ich persönlich ziehen ja eine Drachenschuppenfärbeverordnung einer Gartenschuppenfärbeverordnung in jedem Fall vor...".to_string(),
                vorwort: Some("Vorwort".to_string()),
                zusammenfassung: Some("Zusammenfassungstext kommt hier rein".to_string()),
                schlagworte: Some(vec!["drache".to_string(), "langer text".to_string(), "mächtiggewaltigegon".to_string(), "schuppen".to_string(), "verordnung".to_string()]),
                zp_erstellt: Some(chrono::DateTime::parse_from_rfc3339("1950-01-01T22:01:02+00:00").unwrap().to_utc()),
                zp_referenz: chrono::DateTime::parse_from_rfc3339("1950-01-01T22:01:02+00:00").unwrap().to_utc(),
                zp_modifiziert: chrono::DateTime::parse_from_rfc3339("1950-01-01T22:01:02+00:00").unwrap().to_utc(),
                touched_by: None,
            }
        }
        pub(crate) fn default_stellungnahme() -> models::Dokument {
            models::Dokument{
                api_id: Some(Uuid::from_str("b18bde64-c0ff-eeee-ff0c-deadbeef7777").unwrap()),
                autoren: vec![default_autor_person()],
                hash: "f98d9d6f13635463450d69f6".to_string(),
                drucksnr: None,
                kurztitel: Some("Dokumentblubgedöns".to_string()),
                link: "https://irgendwo.im.nirgendwo.de".to_string(),
                meinung: Some(3u8),
                titel: "Stelungnahme zu: Ganz ausführlicher Titel, der die Schuppenfärbungsverordnung von 2027 zu verändern versucht bevor sie Gesetz wird".to_string(),
                typ: models::Doktyp::Stellungnahme,
                volltext: "Nee, ich denk mir hier keinen Volltext aus. Das wär wirklich viel zu lang. Vor allem zu einer Schuppenfärbeverordnung aus der Zukunft! Soo lächerlich. 
                Natürlich mal wieder Klassiker, dass die hier \"Schuppen\" und nicht \"Fischschuppen\", \"Gartenschuppen\" oder \"Drachenschuppen\" geschrieben haben. Danke Merkel! 
                Ich persönlich ziehen ja eine Drachenschuppenfärbeverordnung einer Gartenschuppenfärbeverordnung in jedem Fall vor...".to_string(),
                vorwort: Some("Stelluingsnahmenvorwort das völlig verschieden von dem Hauptdokument ist".to_string()),
                zusammenfassung: Some("Zusammenfassungstext kommt hier rein".to_string()),
                schlagworte: Some(vec!["drache".to_string(), "langer text".to_string(), "mächtiggewaltigegon".to_string(), "schuppen".to_string(), "verordnung".to_string()]),
                zp_erstellt: Some(chrono::DateTime::parse_from_rfc3339("1950-01-01T22:01:02+00:00").unwrap().to_utc()),
                zp_referenz: chrono::DateTime::parse_from_rfc3339("1950-01-01T22:01:02+00:00").unwrap().to_utc(),
                zp_modifiziert: chrono::DateTime::parse_from_rfc3339("1950-01-01T22:01:02+00:00").unwrap().to_utc(),
                touched_by: None,
            }
        }
        pub(crate) fn default_autor_person() -> models::Autor {
            models::Autor {
                fachgebiet: None,
                lobbyregister: None,
                organisation: "Ministerium der Magie".to_string(),
                person: Some("Harald Maria Töpfer".to_string()),
            }
        }
        pub(crate) fn default_autor_institution() -> models::Autor {
            models::Autor {
                fachgebiet: None,
                lobbyregister: None,
                organisation: "Mysterium der Ministerien".to_string(),
                person: None,
            }
        }
        pub(crate) fn default_autor_experte() -> models::Autor {
            models::Autor {
                person: Some("Karl Preis".to_string()),
                organisation: "Kachelofenbau Hannes".to_string(),
                fachgebiet: Some("Kachelofenbau".to_string()),
                lobbyregister: None,
            }
        }
        pub(crate) fn default_autor_lobby() -> models::Autor {
            models::Autor {
                fachgebiet: None,
                lobbyregister: Some(
                    "https://lobbyregister.beispiel/heinzpeter-karlsbader-ff878f".to_string(),
                ),
                organisation: "Kachelofenzerstörung Heinzelfrau".to_string(),
                person: Some("Heinz-Peter Karlsbader".to_string()),
            }
        }
        pub(crate) fn default_sitzung() -> models::Sitzung {
            models::Sitzung {
                api_id: Some(Uuid::from_str("b18bde64-c0ff-eeee-ff0c-deadbeef9999").unwrap()),
                touched_by: None,
                titel: Some("Klogespräche und -lektüre im 22. Jhd.".to_string()),
                termin: chrono::DateTime::parse_from_rfc3339("1950-01-01T22:01:02+00:00")
                    .unwrap()
                    .to_utc(),
                gremium: default_gremium(),
                nummer: 42,
                public: true,
                link: Some("https://klogefueh.le".to_string()),
                tops: vec![default_top()],
                dokumente: Some(vec![default_dokument()]),
                experten: Some(vec![default_autor_experte()]),
            }
        }
        pub(crate) fn default_top() -> models::Top {
            models::Top {
                dokumente: Some(vec![models::StationDokumenteInner::Dokument(Box::new(
                    default_dokument(),
                ))]),
                nummer: 1,
                titel: "Lektüre und Haptik".to_string(),
                vorgang_id: None,
            }
        }
    }
    struct Scenario {
        context: Vec<models::Vorgang>,
        object: models::Vorgang,
        expected: Vec<models::Vorgang>,
        shouldfail: bool,
        name: &'static str,
    }
    fn xor(b1: bool, b2: bool) -> bool {
        return b1 && !b2 || b2 && !b1;
    }
    impl Scenario {
        async fn run(&self) -> Result<()> {
            let server = self.setup().await?;
            self.build_context(&server).await?;
            self.place_object(&server).await?;
            self.check_result(&server).await?;
            self.teardown(&server).await?;
            Ok(())
        }
        async fn setup(&self) -> Result<LTZFServer> {
            let dburl = std::env::var("DATABASE_URL")
                .expect("Expected to find working DATABASE_URL for testing");
            let config = crate::Configuration {
                mail_server: None,
                mail_user: None,
                mail_password: None,
                mail_sender: None,
                mail_recipient: None,
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
            let query = format!(
                "CREATE DATABASE \"testing_{}\" WITH OWNER 'ltzf-user';",
                self.name
            );
            sqlx::query(&dropquery)
                .execute(&master_server.sqlx_db)
                .await?;
            sqlx::query(&query).execute(&master_server.sqlx_db).await?;

            let db_url = config
                .db_url
                .replace("5432/ltzf", &format!("5432/testing_{}", self.name));
            let oconfig = crate::Configuration {
                db_url: db_url.clone(),
                ..config
            };
            let out_server = LTZFServer {
                config: oconfig,
                mailbundle: None,
                sqlx_db: sqlx::postgres::PgPool::connect(&db_url).await?,
            };
            sqlx::migrate!().run(&out_server.sqlx_db).await?;
            Ok(out_server)
        }

        async fn teardown(&self, server: &LTZFServer) -> Result<()> {
            let dburl = std::env::var("DATABASE_URL")
                .expect("Expected to find working DATABASE_URL for testing");
            let config = crate::Configuration {
                mail_server: None,
                mail_user: None,
                mail_password: None,
                mail_sender: None,
                mail_recipient: None,
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
                super::run_integration(obj, Uuid::nil(), server).await?;
            }
            Ok(())
        }
        async fn place_object(&self, server: &LTZFServer) -> Result<()> {
            super::run_integration(&self.object, Uuid::nil(), server).await?;
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
                0,
                PaginationResponsePart::DEFAULT_PER_PAGE,
                &mut tx,
            )
            .await
            .unwrap();
            tx.commit().await?;

            for expected in self.expected.iter() {
                let mut found = false;
                for db_out in db_vorgangs.1.iter() {
                    if db_out == expected {
                        found = true;
                        break;
                    } else if xor(db_out.api_id != expected.api_id, self.shouldfail) {
                        std::fs::write(
                            format!("tests/{}_dumpa.json", self.name),
                            dump_objects(&expected, &db_out),
                        )
                        .unwrap();
                        assert!(
                            false,
                            "Differing object have the same api id: `{}`. Difference:\n{}",
                            db_out.api_id,
                            crate::db::merge::display_strdiff(
                                &serde_json::to_string_pretty(expected).unwrap(),
                                &serde_json::to_string_pretty(db_out).unwrap()
                            )
                        );
                    }
                }
                if xor(found, self.shouldfail) {
                    std::fs::write(
                        format!("tests/{}_dump.json", self.name),
                        serde_json::to_string_pretty(expected).unwrap(),
                    )
                    .unwrap();
                }
                let default_vorgang = generate::default_vorgang();
                assert!(
                    found,
                    "({}): Expected to find Vorgang with api_id `{}`, but was not present in the output set, which contained: {:?}.\n\nDetails(Output Set):\n{:#?}",
                    self.name,
                    expected.api_id,
                    db_vorgangs
                        .1
                        .iter()
                        .map(|e| e.api_id)
                        .collect::<Vec<uuid::Uuid>>(),
                    db_vorgangs
                        .1
                        .iter()
                        .map(|v| {
                            println!(
                                "{}\nDifference to Default(this is 'g'):\n{}\nDefault Vorgang:\n{}",
                                &serde_json::to_string_pretty(v).unwrap(),
                                crate::db::merge::display_strdiff(
                                    &serde_json::to_string_pretty(&default_vorgang).unwrap(),
                                    &serde_json::to_string_pretty(v).unwrap()
                                ),
                                &serde_json::to_string_pretty(&default_vorgang).unwrap()
                            );
                            "object, see stdout".to_string()
                        })
                        .collect::<Vec<String>>()
                );
            }

            assert!(
                self.expected.len() == db_vorgangs.1.len(),
                "({}): Mismatch between the length of the expected set and the output set: {} (e) vs {} (o)\nOutput Set: {:#?}",
                self.name,
                self.expected.len(),
                db_vorgangs.1.len(),
                db_vorgangs
            );
            // check if both lists are equal
            let mut exp_sorted = self.expected.clone();
            let mut db_sorted = db_vorgangs.1.clone();
            exp_sorted.sort_by(|a, b| a.api_id.cmp(&b.api_id));
            db_sorted.sort_by(|a, b| a.api_id.cmp(&b.api_id));
            assert_eq!(exp_sorted, db_sorted);
            Ok(())
        }
    }

    fn dump_objects<T: serde::Serialize, S: serde::Serialize>(expected: &T, actual: &S) -> String {
        format!(
            "{{ \"expected-object\" : {},\n\"actual-object\" : {}}}",
            serde_json::to_string_pretty(expected).unwrap(),
            serde_json::to_string_pretty(actual).unwrap()
        )
    }

    // one in, again one in, one out
    #[tokio::test]
    async fn test_idempotenz() {
        let vg = generate::default_vorgang();
        let scenario = Scenario {
            context: vec![vg.clone()],
            object: vg.clone(),
            expected: vec![vg],
            name: "idempotenz",
            shouldfail: false,
        };
        scenario.run().await.unwrap();
    }
    #[tokio::test]
    async fn test_merge_matching_ids() {
        todo!()
    }
    #[tokio::test]
    async fn test_merge_matching_vorwort() {
        todo!()
    }
    #[tokio::test]
    async fn test_link_ini_ids_merging() {
        let vg = generate::default_vorgang();
        let mut vg_mod = vg.clone();
        vg_mod.links = Some(vec!["https://example.com".to_string()]);
        vg_mod.ids = Some(vec![models::VgIdent {
            id: "einzigartig".to_string(),
            typ: models::VgIdentTyp::Initdrucks,
        }]);
        vg_mod.initiatoren = vec![models::Autor {
            person: Some("Max Mustermann".to_string()),
            organisation: "Musterorganisation".to_string(),
            fachgebiet: Some("Musterfachgebiet".to_string()),
            lobbyregister: Some("Musterlobbyregister".to_string()),
        }];

        let vg_exp = vg.clone();
        vg_exp.links = Some(); //merged
        let scenario = Scenario {
            context: vec![vg.clone()],
            object: vg_mod.clone(),
            expected: vec![vg_mod],
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
            expected: vec![vg_mod],
            name: "weak_prop_change_override",
            shouldfail: false,
        };
        scenario.run().await.unwrap();
    }
    #[tokio::test]
    async fn test_not_merged_but_separate() {
        todo!()
    }
    #[tokio::test]
    async fn test_schlagwort_duplicate_elimination() {
        todo!()
    }
    #[tokio::test]
    async fn test_schlagwort_formatting() {
        todo!()
    }

    #[tokio::test]
    async fn test_station_merging_on_weak_property_changes() {
        todo!()
    }
    #[tokio::test]
    async fn test_dokument_merging_on_weak_property_changes() {
        todo!()
    }
}
