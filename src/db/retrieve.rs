use std::str::FromStr;

use crate::error::*;
use crate::utils::as_option;
use openapi::models;
use uuid::Uuid;

pub async fn vorgang_by_id(
    id: i32,
    executor: &mut sqlx::PgTransaction<'_>,
) -> Result<models::Vorgang> {
    let pre_vg = sqlx::query!(
        "SELECT v.*, vt.value FROM vorgang v
        INNER JOIN vorgangstyp vt ON vt.id = v.typ
        WHERE v.id = $1",
        id
    )
    .fetch_one(&mut **executor)
    .await?;

    let links = sqlx::query!(
        "SELECT link FROM rel_vorgang_links WHERE vg_id = $1 ORDER BY link ASC",
        id
    )
    .map(|row| row.link)
    .fetch_all(&mut **executor)
    .await?;

    let init_inst = sqlx::query!(
        "SELECT a.* FROM rel_vorgang_init 
        INNER JOIN autor a ON a.id = in_id
        WHERE vg_id = $1 ORDER BY organisation ASC",
        id
    )
    .map(|row| models::Autor {
        fachgebiet: row.fachgebiet,
        lobbyregister: row.lobbyregister,
        organisation: row.organisation,
        person: row.person,
    })
    .fetch_all(&mut **executor)
    .await?;

    let ids = sqlx::query!(
        "
    SELECT value as typ, identifikator as ident 
    FROM rel_vorgang_ident r
    INNER JOIN vg_ident_typ t ON t.id = r.typ
    WHERE r.vg_id = $1
    ORDER BY ident ASC",
        id
    )
    .map(|row| models::VgIdent {
        typ: models::VgIdentTyp::from_str(row.typ.as_str()).unwrap_or_else(|_| {
            panic!(
                "Could not convert database value `{}`into VgIdentTyp Variant",
                row.typ
            )
        }),
        id: row.ident,
    })
    .fetch_all(&mut **executor)
    .await?;

    let station_ids = sqlx::query!("SELECT id FROM station WHERE vg_id = $1", id)
        .map(|row| row.id)
        .fetch_all(&mut **executor)
        .await?;

    let mut stationen = vec![];
    for sid in station_ids {
        stationen.push(station_by_id(sid, executor).await?);
    }
    stationen.sort_by(|a, b| a.zp_start.cmp(&b.zp_start));

    Ok(models::Vorgang {
        api_id: pre_vg.api_id,
        titel: pre_vg.titel,
        kurztitel: pre_vg.kurztitel,
        wahlperiode: pre_vg.wahlperiode as u32,
        verfassungsaendernd: pre_vg.verfaend,
        typ: models::Vorgangstyp::from_str(pre_vg.value.as_str())
            .map_err(|e| DataValidationError::InvalidEnumValue { msg: e })?,
        initiatoren: init_inst,
        ids: Some(ids),
        links: Some(links),
        stationen,
    })
}

pub async fn station_by_id(
    id: i32,
    executor: &mut sqlx::PgTransaction<'_>,
) -> Result<models::Station> {
    let dokids = sqlx::query!(
        "SELECT dok_id FROM rel_station_dokument WHERE stat_id = $1",
        id
    )
    .map(|r| r.dok_id)
    .fetch_all(&mut **executor)
    .await?;
    let mut doks = Vec::with_capacity(dokids.len());
    for did in dokids {
        doks.push(dokument_by_id(did, executor).await?.into());
    }
    doks.sort_by(|a, b| match (a, b) {
        (models::DokRef::Dokument(a), models::DokRef::Dokument(b)) => a.link.cmp(&b.link),
        _ => {
            unreachable!("If this is the case document extraction failed")
        }
    });
    let stlid = sqlx::query!("SELECT dok_id FROM rel_station_stln WHERE stat_id = $1", id)
        .map(|r| r.dok_id)
        .fetch_all(&mut **executor)
        .await?;
    let mut stellungnahmen = Vec::with_capacity(stlid.len());
    for sid in stlid {
        stellungnahmen.push(dokument_by_id(sid, executor).await?);
    }
    stellungnahmen.sort_by(|a, b| a.link.cmp(&b.link));
    let sw = sqlx::query!(
        "SELECT DISTINCT(value) FROM rel_station_schlagwort r
        LEFT JOIN schlagwort sw ON sw.id = r.sw_id
        WHERE r.stat_id = $1
        ORDER BY value ASC",
        id
    )
    .map(|sw| sw.value)
    .fetch_all(&mut **executor)
    .await?;

    let add_links = sqlx::query!("SELECT link FROM rel_station_link WHERE stat_id = $1", id)
        .map(|r| r.link)
        .fetch_all(&mut **executor)
        .await?;
    let temp_stat = sqlx::query!(
        "SELECT s.*, p.value as parlv, st.value as stattyp
        FROM station s
        INNER JOIN parlament p ON p.id = s.p_id
        INNER JOIN stationstyp st ON st.id = s.typ
        WHERE s.id=$1",
        id
    )
    .fetch_one(&mut **executor)
    .await?;

    let gremium = sqlx::query!(
        "
    SELECT p.value, g.name, g.wp, 
    g.link FROM gremium g INNER JOIN parlament p on p.id = g.parl
        WHERE g.id = $1",
        temp_stat.gr_id
    )
    .map(|x| models::Gremium {
        name: x.name,
        wahlperiode: x.wp as u32,
        parlament: models::Parlament::from_str(&x.value).unwrap(),
        link: x.link,
    })
    .fetch_optional(&mut **executor)
    .await?;

    Ok(models::Station {
        parlament: models::Parlament::from_str(temp_stat.parlv.as_str())
            .map_err(|e| DataValidationError::InvalidEnumValue { msg: e })?,
        typ: models::Stationstyp::from_str(temp_stat.stattyp.as_str())
            .map_err(|e| DataValidationError::InvalidEnumValue { msg: e })?,
        dokumente: doks,
        schlagworte: as_option(sw),
        stellungnahmen: as_option(stellungnahmen),
        zp_start: temp_stat.zp_start,
        zp_modifiziert: Some(temp_stat.zp_modifiziert),

        trojanergefahr: temp_stat.trojanergefahr.map(|x| x as u8),
        titel: temp_stat.titel,
        gremium,
        api_id: Some(temp_stat.api_id),
        link: temp_stat.link,
        additional_links: as_option(add_links),
        gremium_federf: temp_stat.gremium_isff,
    })
}

pub async fn dokument_by_id(
    id: i32,
    executor: &mut sqlx::PgTransaction<'_>,
) -> Result<models::Dokument> {
    tracing::debug!("Fetching dokument with id {}", id);
    let rec = sqlx::query!(
        "SELECT d.*, value as typ_value FROM dokument d
        INNER JOIN dokumententyp dt ON dt.id = d.typ
        WHERE d.id = $1",
        id
    )
    .fetch_one(&mut **executor)
    .await?;
    let schlagworte = sqlx::query!(
        "SELECT DISTINCT value 
        FROM rel_dok_schlagwort r
        LEFT JOIN schlagwort sw ON sw.id = r.sw_id
        WHERE dok_id = $1
        ORDER BY value ASC",
        id
    )
    .map(|r| r.value)
    .fetch_all(&mut **executor)
    .await?;
    let autoren = sqlx::query!(
        "SELECT a.* FROM rel_dok_autor 
        INNER JOIN autor a ON a.id = aut_id
        WHERE dok_id = $1 
        ORDER BY organisation ASC",
        id
    )
    .map(|r| models::Autor {
        person: r.person,
        organisation: r.organisation,
        lobbyregister: r.lobbyregister,
        fachgebiet: r.fachgebiet,
    })
    .fetch_all(&mut **executor)
    .await?;

    Ok(models::Dokument {
        api_id: Some(rec.api_id),
        titel: rec.titel,
        kurztitel: rec.kurztitel,
        vorwort: rec.vorwort,
        volltext: rec.volltext,

        zp_erstellt: rec.zp_created,
        zp_modifiziert: rec.zp_lastmod,
        zp_referenz: rec.zp_referenz,

        link: rec.link,
        hash: rec.hash,
        meinung: rec.meinung.map(|x| x as u8),
        zusammenfassung: rec.zusammenfassung,
        schlagworte: as_option(schlagworte),
        autoren,
        typ: models::Doktyp::from_str(rec.typ_value.as_str())
            .map_err(|e| DataValidationError::InvalidEnumValue { msg: e })?,
        drucksnr: rec.drucksnr,
    })
}

/// the crucial part is how to find out which vg are connected to a DRCKS
/// if there exists a station which contains a document mentioned in the top, its vorgang is connected
pub async fn top_by_id(id: i32, tx: &mut sqlx::PgTransaction<'_>) -> Result<models::Top> {
    let scaffold = sqlx::query!("SELECT titel, nummer FROM top WHERE id = $1", id)
        .fetch_one(&mut **tx)
        .await?;
    // ds
    let dids = sqlx::query!("SELECT dok_id FROM tops_doks td WHERE top_id = $1", id)
        .map(|r| r.dok_id)
        .fetch_all(&mut **tx)
        .await?;
    let mut doks = vec![];
    for did in dids {
        doks.push(dokument_by_id(did, tx).await?.into());
    }
    doks.sort_by(|a, b| match (a, b) {
        (models::DokRef::Dokument(a), models::DokRef::Dokument(b)) => a.link.cmp(&b.link),
        _ => {
            unreachable!("If this is the case document extraction failed")
        }
    });
    // vgs
    let vgs = sqlx::query!(
        "
    SELECT DISTINCT(v.api_id) FROM station s    -- alle vorgänge von stationen,
INNER JOIN vorgang v ON v.id = s.vg_id
WHERE
EXISTS ( 									-- mit denen mindestens ein dokument assoziiert ist, dass hier auftaucht
	SELECT 1 FROM rel_station_dokument rsd 
	INNER JOIN tops_doks td ON td.dok_id = rsd.dok_id
	WHERE td.top_id = $1 AND rsd.stat_id = s.id
) OR EXISTS(			             		-- mit denen mindestens ein dokument assoziiert ist, dass hier auftaucht
	SELECT 1 FROM rel_station_stln rss
	INNER JOIN tops_doks td ON td.dok_id = rss.dok_id
	WHERE td.top_id = $1 AND rss.stat_id = s.id
)
    ORDER BY api_id ASC",
        id
    )
    .map(|r| r.api_id)
    .fetch_all(&mut **tx)
    .await?;

    Ok(models::Top {
        nummer: scaffold.nummer as u32,
        titel: scaffold.titel,
        dokumente: as_option(doks),
        vorgang_id: as_option(vgs),
    })
}

pub async fn sitzung_by_id(id: i32, tx: &mut sqlx::PgTransaction<'_>) -> Result<models::Sitzung> {
    let scaffold = sqlx::query!(
        "SELECT a.api_id, a.public, a.termin, p.value as plm, a.link as as_link, a.titel, a.nummer,
        g.name as grname, g.wp, g.link as gr_link FROM sitzung a
        INNER JOIN gremium g ON g.id = a.gr_id
        INNER JOIN parlament p ON p.id = g.parl 
        WHERE a.id = $1",
        id
    )
    .fetch_one(&mut **tx)
    .await?;
    // tops
    let topids = sqlx::query!(
        "SELECT * FROM top t WHERE t.sid = $1 ORDER BY titel ASC",
        id
    )
    .map(|r| r.id)
    .fetch_all(&mut **tx)
    .await?;
    let mut tops = vec![];
    for top in &topids {
        tops.push(top_by_id(*top, tx).await?);
    }
    // experten
    let experten = sqlx::query!(
        "SELECT a.* FROM rel_sitzung_experten rae 
        INNER JOIN autor a ON rae.eid = a.id
		WHERE rae.sid = $1
        ORDER BY a.organisation ASC, a.person ASC",
        id
    )
    .map(|r| models::Autor {
        fachgebiet: r.fachgebiet,
        lobbyregister: r.lobbyregister,
        organisation: r.organisation,
        person: r.person,
    })
    .fetch_all(&mut **tx)
    .await?;

    let dids = sqlx::query!(
        "SELECT dok_id from reL_station_dokument WHERE stat_id = $1",
        id
    )
    .map(|r| r.dok_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut doks = vec![];
    for d in dids {
        doks.push(dokument_by_id(d, tx).await?);
    }

    Ok(models::Sitzung {
        api_id: Some(scaffold.api_id),
        nummer: scaffold.nummer as u32,
        titel: scaffold.titel,
        public: scaffold.public,
        termin: scaffold.termin,
        gremium: models::Gremium {
            name: scaffold.grname,
            link: scaffold.gr_link,
            wahlperiode: scaffold.wp as u32,
            parlament: models::Parlament::from_str(&scaffold.plm).unwrap(),
        },
        tops,
        link: scaffold.as_link,
        experten: as_option(experten),
        dokumente: as_option(doks),
    })
}

pub struct SitzungFilterParameters {
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    pub parlament: Option<models::Parlament>,
    pub wp: Option<u32>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub vgid: Option<Uuid>,
    pub gremium_like: Option<String>,
}
pub async fn sitzung_by_param(
    params: &SitzungFilterParameters,
    tx: &mut sqlx::PgTransaction<'_>,
) -> Result<Vec<models::Sitzung>> {
    let as_list = sqlx::query!(
        "
      WITH pre_table AS (
        SELECT a.id, MAX(a.termin) as lastmod FROM  sitzung a
		INNER JOIN gremium g ON g.id = a.gr_id
		INNER JOIN parlament p ON p.id = g.parl
		WHERE p.value = COALESCE($1, p.value)
		AND g.wp = 		COALESCE($2, g.wp)
        AND (SIMILARITY(g.name, $7) > 0.66 OR $7 IS NULL)
        GROUP BY a.id
        ORDER BY lastmod
        ),
	vgref AS   (
		SELECT p.id, v.api_id FROM pre_table p
		INNER JOIN top on top.sid = p.id
		INNER JOIN tops_doks ON tops_doks.top_id = top.id
		LEFT JOIN rel_station_dokument rsd ON rsd.dok_id = tops_doks.dok_id
		LEFT JOIN rel_station_stln rss ON rss.dok_id = tops_doks.dok_id
		INNER JOIN station s ON s.id = rsd.stat_id OR s.id = rss.stat_id
		INNER JOIN vorgang v ON s.vg_id = v.id
	)

SELECT * FROM pre_table WHERE
lastmod > COALESCE($3, CAST('1940-01-01T20:20:20Z' as TIMESTAMPTZ)) AND
lastmod < COALESCE($4, NOW()) AND
(CAST ($8 AS UUID) IS NULL OR EXISTS (SELECT 1 FROM vgref WHERE pre_table.id = vgref.id AND vgref.api_id = COALESCE($8, vgref.api_id)))
ORDER BY pre_table.lastmod ASC
OFFSET COALESCE($5, 0) 
LIMIT COALESCE($6, 64)",
        params.parlament.map(|p| p.to_string()),
        params.wp.map(|x|x as i32),
        params.since,
        params.until,
        params.offset.map(|x|x as i32),
        params.limit.map(|x|x as i32),
        params.gremium_like,
        params.vgid
    )
    .map(|r| r.id)
    .fetch_all(&mut **tx)
    .await?;
    let mut vector = Vec::with_capacity(as_list.len());
    for id in as_list {
        vector.push(super::retrieve::sitzung_by_id(id, tx).await?);
    }
    Ok(vector)
}

#[derive(Debug)]
pub struct VGGetParameters {
    pub limit: Option<i32>,
    pub offset: Option<i32>,
    pub lower_date: Option<chrono::DateTime<chrono::Utc>>,
    pub upper_date: Option<chrono::DateTime<chrono::Utc>>,
    pub parlament: Option<models::Parlament>,
    pub wp: Option<i32>,
    pub inipsn: Option<String>,
    pub iniorg: Option<String>,
    pub inifch: Option<String>,
    pub vgtyp: Option<models::Vorgangstyp>,
}
pub async fn vorgang_by_parameter(
    params: VGGetParameters,
    executor: &mut sqlx::PgTransaction<'_>,
) -> Result<Vec<models::Vorgang>> {
    let vg_list = sqlx::query!(
        "WITH pre_table AS (
        SELECT vorgang.id, MAX(station.zp_start) as lastmod FROM vorgang
            INNER JOIN vorgangstyp vt ON vt.id = vorgang.typ
            LEFT JOIN station ON station.vg_id = vorgang.id
			INNER JOIN parlament on parlament.id = station.p_id
            WHERE TRUE
            AND vorgang.wahlperiode = COALESCE($1, vorgang.wahlperiode)
            AND vt.value = COALESCE($2, vt.value)
			AND parlament.value= COALESCE($3, parlament.value)
			AND (CAST($4 as text) IS NULL OR EXISTS(SELECT 1 FROM rel_vorgang_init rvi INNER JOIN autor a ON a.id = rvi.in_id WHERE a.person = $4))
			AND (CAST($5 as text) IS NULL OR EXISTS(SELECT 1 FROM rel_vorgang_init rvi INNER JOIN autor a ON a.id = rvi.in_id WHERE a.organisation = $5))
			AND (CAST($6 as text) IS NULL OR EXISTS(SELECT 1 FROM rel_vorgang_init rvi INNER JOIN autor a ON a.id = rvi.in_id WHERE a.fachgebiet = $6))
        GROUP BY vorgang.id
        ORDER BY lastmod
        )
SELECT * FROM pre_table WHERE
lastmod > COALESCE($7, CAST('1940-01-01T20:20:20Z' as TIMESTAMPTZ)) 
AND lastmod < COALESCE($8, NOW())
ORDER BY pre_table.lastmod ASC
OFFSET COALESCE($9, 0) LIMIT COALESCE($10, 64)
",params.wp, params.vgtyp.map(|x|x.to_string()),
params.parlament.map(|p|p.to_string()),
params.inipsn, params.iniorg, params.inifch, params.lower_date, params.upper_date, params.offset,
    params.limit)
    .map(|r|r.id)
    .fetch_all(&mut **executor).await?;

    let mut vector = Vec::with_capacity(vg_list.len());
    for id in vg_list {
        vector.push(super::retrieve::vorgang_by_id(id, executor).await?);
    }
    Ok(vector)
}
