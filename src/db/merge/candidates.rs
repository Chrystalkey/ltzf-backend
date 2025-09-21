use crate::LTZFServer;
use crate::Result;
use crate::db::merge::MatchState;
use openapi::models;
use uuid::Uuid;

/// this function determines what means "matching enough".
/// 1. wenn api_id matcht
/// 2. wenn wp, typ und mindestens ein identifikator matchen
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
		(EXISTS (SELECT 1 FROM UNNEST($2::text[], $3::text[]) as eingabe(ident, typ), db_id_table WHERE  -- eine übereinstimmende ID existiert
			db_id_table.vg_id = vorgang.id AND
			eingabe.ident = db_id_table.ident AND
			eingabe.typ = db_id_table.idt_str)
		)
	);",
    model.api_id, &ident_t[..], &identt_t[..], model.wahlperiode as i32,
    srv.guard_ts(model.typ, model.api_id, obj)?)
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
/// 2. wenn vorgang, typ und gremium matchen und mindestens ein Dokument gleich ist
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
    let (gr_name, gr_wp, gr_parl) = (
        model.gremium.name.clone(),
        model.gremium.wahlperiode as i32,
        model.gremium.parlament.to_string(),
    );
    let result = sqlx::query!(
        "SELECT s.id, s.api_id FROM station s
    INNER JOIN stationstyp st ON st.id=s.typ
    INNER JOIN gremium g ON g.id=s.gr_id
    INNER JOIN parlament p ON p.id = g.parl
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

/// wenn gleich:
/// api_id OR hash OR (typ AND drucksNr AND zp_referenz)
pub async fn dokument_merge_candidates(
    model: &models::Dokument,
    executor: impl sqlx::PgExecutor<'_>,
    srv: &LTZFServer,
) -> Result<MatchState<i32>> {
    let dids = sqlx::query!(
        "SELECT d.id FROM dokument d 
        INNER JOIN dokumententyp dt ON dt.id = d.typ 
        WHERE 
        d.hash = $1 OR
        d.api_id = $2 OR
        (d.drucksnr = $3 AND dt.value = $4 AND ($5 BETWEEN (d.zp_referenz-'12 hours'::interval) AND (d.zp_referenz+'12 hours'::interval)))",
        model.hash,
        model.api_id,
        model.drucksnr,
        srv.guard_ts(
            model.typ,
            model.api_id.unwrap_or(Uuid::nil()),
            "dok_merge_candidates"
        )?,
        model.zp_referenz
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

#[cfg(test)]
mod candid_test {
    use super::*;
    use crate::api::auth;
    use crate::utils::test::generate::default_vorgang;
    use crate::{db::merge::MatchState, utils::test::generate};
    use axum::http::Method;
    use axum_extra::extract::{CookieJar, Host};
    use chrono::DateTime;
    use openapi::apis::data_administration_vorgang::DataAdministrationVorgang;
    use openapi::models;

    #[tokio::test]
    async fn vorgang_test() {
        let srv = generate::setup_server("test_vorgang_candidates")
            .await
            .unwrap();
        let vgs = vec![
            generate::vorgang_with_seed(0),
            generate::vorgang_with_seed(1),
            generate::vorgang_with_seed(2),
            generate::vorgang_with_seed(3),
            generate::vorgang_with_seed(4),
            generate::vorgang_with_seed(5),
            generate::vorgang_with_seed(6),
        ];
        // insert vorgang 1,2,3, ...
        for vg in vgs.iter() {
            let r = srv
                .vorgang_id_put(
                    &Method::PUT,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(auth::APIScope::Admin, 1),
                    &models::VorgangIdPutPathParams {
                        vorgang_id: vg.api_id,
                    },
                    vg,
                )
                .await
                .unwrap();
            assert!(matches!(r, openapi::apis::data_administration_vorgang::VorgangIdPutResponse::Status201_Created { .. }));
        }
        let mut tx = srv.sqlx_db.begin().await.unwrap();
        // check wether all conditions are "enough" to find a specific station
        let mod_v0 = models::Vorgang {
            api_id: uuid::Uuid::nil(),
            ..vgs[0].clone()
        };
        // matching on exactly one if only differing el is the api_id
        let candidate = super::vorgang_merge_candidates(&mod_v0, &mut *tx, &srv)
            .await
            .unwrap();
        assert!(matches!(candidate, MatchState::ExactlyOne(_)));
        // check wether insufficient uniqueness conditions yield appropriate results:
        // no match for something not in the db
        let candidate = super::vorgang_merge_candidates(&default_vorgang(), &mut *tx, &srv)
            .await
            .unwrap();
        assert!(matches!(candidate, MatchState::NoMatch));
        // ambiguous match for ambiguous conditions
        // TODO
    }
    #[tokio::test]
    async fn station_test() {
        let _srv = generate::setup_server("test_station_candidates")
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn dokument_test() {
        let srv = generate::setup_server("test_dokument_candidates")
            .await
            .unwrap();
        let vgs = vec![
            models::Vorgang {
                stationen: vec![models::Station {
                    dokumente: vec![models::StationDokumenteInner::Dokument(Box::new(
                        generate::dokument_with_seed(0),
                    ))],
                    ..generate::station_with_seed(0)
                }],
                ..generate::vorgang_with_seed(0)
            },
            generate::vorgang_with_seed(1),
            generate::vorgang_with_seed(2),
            generate::vorgang_with_seed(3),
            generate::vorgang_with_seed(4),
            generate::vorgang_with_seed(5),
            generate::vorgang_with_seed(6),
        ];
        // insert vorgang 1,2,3, ...
        for vg in vgs.iter() {
            let r = srv
                .vorgang_id_put(
                    &Method::PUT,
                    &Host("localhost".to_string()),
                    &CookieJar::new(),
                    &(auth::APIScope::Admin, 1),
                    &models::VorgangIdPutPathParams {
                        vorgang_id: vg.api_id,
                    },
                    vg,
                )
                .await
                .unwrap();
            assert!(matches!(r, openapi::apis::data_administration_vorgang::VorgangIdPutResponse::Status201_Created { .. }));
        }
        let mut tx = srv.sqlx_db.begin().await.unwrap();
        let test_docs = [
            // by api_id
            models::Dokument {
                hash: "91843918479182471".to_string(),
                drucksnr: Some("123/1241204".to_string()),
                zp_referenz: DateTime::parse_from_rfc3339("2025-09-17T12:12:14Z")
                    .unwrap()
                    .to_utc(),
                typ: models::Doktyp::Antwort,
                ..generate::dokument_with_seed(0)
            },
            // by hash
            models::Dokument {
                api_id: Some(uuid::Uuid::now_v7()),
                drucksnr: Some("123/1241204".to_string()),
                zp_referenz: DateTime::parse_from_rfc3339("2025-09-17T12:12:14Z")
                    .unwrap()
                    .to_utc(),
                typ: models::Doktyp::Antwort,
                ..generate::dokument_with_seed(0)
            },
            // by typ, refts, drucksnr
            models::Dokument {
                api_id: Some(uuid::Uuid::now_v7()),
                hash: "91843918479182471".to_string(),
                ..generate::dokument_with_seed(0)
            },
        ];
        for (i, d) in test_docs.iter().enumerate() {
            let r = dokument_merge_candidates(&d, &mut *tx, &srv).await.unwrap();
            assert!(
                matches!(r, MatchState::ExactlyOne(_)),
                "Dok {} was {:?}",
                i,
                r
            );
        }
        // no matching identifiers
        let fail = models::Dokument {
            api_id: Some(uuid::Uuid::now_v7()),
            hash: "91843918479182471".to_string(),
            drucksnr: Some("123/1241204".to_string()),
            zp_referenz: DateTime::parse_from_rfc3339("2025-09-17T12:12:14Z")
                .unwrap()
                .to_utc(),
            typ: models::Doktyp::Antwort,
            ..generate::dokument_with_seed(0)
        };
        let r = dokument_merge_candidates(&fail, &mut *tx, &srv)
            .await
            .unwrap();
        assert!(matches!(r, MatchState::NoMatch));
    }
}
