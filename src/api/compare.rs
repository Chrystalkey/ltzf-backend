use chrono::{DateTime, Utc};
use openapi::models::*;

fn compare_datetime_millis(dt1: &DateTime<Utc>, dt2: &DateTime<Utc>) -> bool {
    dt1.timestamp_millis() == dt2.timestamp_millis()
}

fn compare_dokument(d1: &Dokument, d2: &Dokument) -> bool {
    if d1.api_id != d2.api_id
        || d1.drucksnr != d2.drucksnr
        || d1.typ != d2.typ
        || d1.titel != d2.titel
        || d1.kurztitel != d2.kurztitel
        || d1.vorwort != d2.vorwort
        || d1.volltext != d2.volltext
        || d1.zusammenfassung != d2.zusammenfassung
        || !compare_datetime_millis(&d1.zp_modifiziert, &d2.zp_modifiziert)
        || !compare_datetime_millis(&d1.zp_referenz, &d2.zp_referenz)
        || d1.zp_erstellt.is_some() != d2.zp_erstellt.is_some()
        || (d1.zp_erstellt.is_some()
            && d2.zp_erstellt.is_some()
            && !compare_datetime_millis(
                d1.zp_erstellt.as_ref().unwrap(),
                d2.zp_erstellt.as_ref().unwrap(),
            ))
        || d1.link != d2.link
        || d1.hash != d2.hash
        || d1.meinung != d2.meinung
        || d1.schlagworte.is_some() != d2.schlagworte.is_some()
    {
        return false;
    }
    if d1.schlagworte.is_some() && d2.schlagworte.is_some() {
        let mut sorted_sw1 = d1.schlagworte.clone().unwrap();
        sorted_sw1.sort();
        let mut sorted_sw2 = d2.schlagworte.clone().unwrap();
        sorted_sw2.sort();
        if sorted_sw1 != sorted_sw2 {
            return false;
        }
    }
    // Compare autoren - order independent
    if d1.autoren.len() != d2.autoren.len() {
        return false;
    }
    let mut autoren1 = d1.autoren.clone();
    let mut autoren2 = d2.autoren.clone();
    autoren1.sort_by(|a, b| a.person.cmp(&b.person));
    autoren2.sort_by(|a, b| a.person.cmp(&b.person));
    for (a1, a2) in autoren1.iter().zip(autoren2.iter()) {
        if a1.person != a2.person
            || a1.organisation != a2.organisation
            || a1.fachgebiet != a2.fachgebiet
            || a1.lobbyregister != a2.lobbyregister
        {
            return false;
        }
    }

    true
}

fn compare_top(t1: &Top, t2: &Top) -> bool {
    if t1.nummer != t2.nummer || t1.titel != t2.titel || t1.vorgang_id != t2.vorgang_id {
        return false;
    }

    // Compare dokumente - order independent
    if t1.dokumente.is_some() != t2.dokumente.is_some() {
        return false;
    }
    if let (Some(docs1), Some(docs2)) = (&t1.dokumente, &t2.dokumente) {
        if docs1.len() != docs2.len() {
            return false;
        }
        let mut sorted_docs1 = docs1.clone();
        let mut sorted_docs2 = docs2.clone();
        sorted_docs1.sort_by(|a, b| match (a, b) {
            (DokRef::Dokument(d1), DokRef::Dokument(d2)) => d1.api_id.cmp(&d2.api_id),
            (DokRef::StringRef(s1), DokRef::StringRef(s2)) => s1.value.cmp(&s2.value),
            _ => std::cmp::Ordering::Equal,
        });
        sorted_docs2.sort_by(|a, b| match (a, b) {
            (DokRef::Dokument(d1), DokRef::Dokument(d2)) => d1.api_id.cmp(&d2.api_id),
            (DokRef::StringRef(s1), DokRef::StringRef(s2)) => s1.value.cmp(&s2.value),
            _ => std::cmp::Ordering::Equal,
        });
        for (d1, d2) in sorted_docs1.iter().zip(sorted_docs2.iter()) {
            match (d1, d2) {
                (DokRef::Dokument(doc1), DokRef::Dokument(doc2)) => {
                    if !compare_dokument(doc1, doc2) {
                        return false;
                    }
                }
                (DokRef::StringRef(s1), DokRef::StringRef(s2)) => {
                    if s1 != s2 {
                        return false;
                    }
                }
                _ => return false, // Different variants
            }
        }
    }

    true
}

pub fn compare_sitzung(s1: &Sitzung, s2: &Sitzung) -> bool {
    if s1.api_id != s2.api_id
        || s1.titel != s2.titel
        || !compare_datetime_millis(&s1.termin, &s2.termin)
        || s1.gremium != s2.gremium
        || s1.nummer != s2.nummer
        || s1.public != s2.public
        || s1.link != s2.link
    {
        return false;
    }

    // Compare tops - order independent
    if s1.tops.len() != s2.tops.len() {
        return false;
    }
    let mut tops1 = s1.tops.clone();
    let mut tops2 = s2.tops.clone();
    tops1.sort_by(|a, b| a.nummer.cmp(&b.nummer));
    tops2.sort_by(|a, b| a.nummer.cmp(&b.nummer));
    for (t1, t2) in tops1.iter().zip(tops2.iter()) {
        if !compare_top(t1, t2) {
            return false;
        }
    }

    // Compare dokumente - order independent
    if s1.dokumente.is_some() != s2.dokumente.is_some() {
        return false;
    }
    if let (Some(docs1), Some(docs2)) = (&s1.dokumente, &s2.dokumente) {
        if docs1.len() != docs2.len() {
            return false;
        }
        let mut sorted_docs1 = docs1.clone();
        let mut sorted_docs2 = docs2.clone();
        sorted_docs1.sort_by(|a, b| a.api_id.cmp(&b.api_id));
        sorted_docs2.sort_by(|a, b| a.api_id.cmp(&b.api_id));
        for (d1, d2) in sorted_docs1.iter().zip(sorted_docs2.iter()) {
            if !compare_dokument(d1, d2) {
                return false;
            }
        }
    }

    // Compare experten - order independent
    if s1.experten.is_some() != s2.experten.is_some() {
        return false;
    }
    if let (Some(exp1), Some(exp2)) = (&s1.experten, &s2.experten) {
        if exp1.len() != exp2.len() {
            return false;
        }
        let mut sorted_exp1 = exp1.clone();
        let mut sorted_exp2 = exp2.clone();
        sorted_exp1.sort_by(|a, b| a.person.cmp(&b.person));
        sorted_exp2.sort_by(|a, b| a.person.cmp(&b.person));
        for (e1, e2) in sorted_exp1.iter().zip(sorted_exp2.iter()) {
            if e1.person != e2.person
                || e1.organisation != e2.organisation
                || e1.fachgebiet != e2.fachgebiet
                || e1.lobbyregister != e2.lobbyregister
            {
                return false;
            }
        }
    }

    true
}

pub fn compare_vorgang(vg1: &Vorgang, vg2: &Vorgang) -> bool {
    // Compare basic fields
    if vg1.api_id != vg2.api_id
        || vg1.titel != vg2.titel
        || vg1.kurztitel != vg2.kurztitel
        || vg1.wahlperiode != vg2.wahlperiode
        || vg1.verfassungsaendernd != vg2.verfassungsaendernd
        || vg1.typ != vg2.typ
    {
        return false;
    }

    // Compare optional fields
    if vg1.ids != vg2.ids || vg1.links != vg2.links {
        return false;
    }

    // Compare initiatoren - order independent
    if vg1.initiatoren.len() != vg2.initiatoren.len() {
        return false;
    }
    let mut init1 = vg1.initiatoren.clone();
    let mut init2 = vg2.initiatoren.clone();
    init1.sort_by(|a, b| a.person.cmp(&b.person));
    init2.sort_by(|a, b| a.person.cmp(&b.person));
    for (i1, i2) in init1.iter().zip(init2.iter()) {
        if i1.person != i2.person
            || i1.organisation != i2.organisation
            || i1.fachgebiet != i2.fachgebiet
            || i1.lobbyregister != i2.lobbyregister
        {
            return false;
        }
    }

    // Compare stationen with special date handling - order independent
    if vg1.stationen.len() != vg2.stationen.len() {
        return false;
    }
    let mut stat1 = vg1.stationen.clone();
    let mut stat2 = vg2.stationen.clone();
    stat1.sort_by(|a, b| a.api_id.cmp(&b.api_id));
    stat2.sort_by(|a, b| a.api_id.cmp(&b.api_id));
    for (s1, s2) in stat1.iter().zip(stat2.iter()) {
        if s1.api_id != s2.api_id
            || s1.titel != s2.titel
            || !compare_datetime_millis(&s1.zp_start, &s2.zp_start)
            || s1.zp_modifiziert.is_some() != s2.zp_modifiziert.is_some()
            || (s1.zp_modifiziert.is_some()
                && s2.zp_modifiziert.is_some()
                && !compare_datetime_millis(
                    s1.zp_modifiziert.as_ref().unwrap(),
                    s2.zp_modifiziert.as_ref().unwrap(),
                ))
            || s1.gremium != s2.gremium
            || s1.gremium_federf != s2.gremium_federf
            || s1.link != s2.link
            || s1.parlament != s2.parlament
            || s1.typ != s2.typ
            || s1.trojanergefahr != s2.trojanergefahr
            || s1.schlagworte != s2.schlagworte
            || s1.additional_links != s2.additional_links
        {
            return false;
        }

        // Compare dokumente - order independent
        if s1.dokumente.len() != s2.dokumente.len() {
            return false;
        }
        let mut docs1 = s1.dokumente.clone();
        let mut docs2 = s2.dokumente.clone();
        docs1.sort_by(|a, b| match (a, b) {
            (DokRef::Dokument(d1), DokRef::Dokument(d2)) => d1.api_id.cmp(&d2.api_id),
            (DokRef::StringRef(s1), DokRef::StringRef(s2)) => s1.value.cmp(&s2.value),
            _ => std::cmp::Ordering::Equal,
        });
        docs2.sort_by(|a, b| match (a, b) {
            (DokRef::Dokument(d1), DokRef::Dokument(d2)) => d1.api_id.cmp(&d2.api_id),
            (DokRef::StringRef(s1), DokRef::StringRef(s2)) => s1.value.cmp(&s2.value),
            _ => std::cmp::Ordering::Equal,
        });
        for (d1, d2) in docs1.iter().zip(docs2.iter()) {
            match (d1, d2) {
                (DokRef::Dokument(doc1), DokRef::Dokument(doc2)) => {
                    if !compare_dokument(doc1, doc2) {
                        return false;
                    }
                }
                (DokRef::StringRef(s1), DokRef::StringRef(s2)) => {
                    if s1 != s2 {
                        return false;
                    }
                }
                _ => return false, // Different variants
            }
        }

        // Compare stellungnahmen - order independent
        if s1.stellungnahmen.is_some() != s2.stellungnahmen.is_some() {
            return false;
        }
        if let (Some(st1), Some(st2)) = (&s1.stellungnahmen, &s2.stellungnahmen) {
            if st1.len() != st2.len() {
                return false;
            }
            let mut sorted_st1 = st1.clone();
            let mut sorted_st2 = st2.clone();
            sorted_st1.sort_by(|a, b| a.api_id.cmp(&b.api_id));
            sorted_st2.sort_by(|a, b| a.api_id.cmp(&b.api_id));
            for (d1, d2) in sorted_st1.iter().zip(sorted_st2.iter()) {
                if !compare_dokument(d1, d2) {
                    return false;
                }
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Timelike};
    use openapi::models;

    #[test]
    fn test_compare_datetime_seconds() {
        // Test identical datetimes
        let dt1 = create_test_datetime();
        let dt2 = dt1.clone();
        assert!(compare_datetime_millis(&dt1, &dt2));

        // Test datetimes with same milliseconds but different nanoseconds
        let dt1 = create_test_datetime_with_nanos(100_000);
        let dt2 = create_test_datetime_with_nanos(200_000);
        // Both should have same millisecond value despite different nanoseconds
        assert!(compare_datetime_millis(&dt1, &dt2));

        // Test different milliseconds
        let dt1 = Utc::now();
        let dt2 = dt1 + Duration::seconds(1);
        assert!(!compare_datetime_millis(&dt1, &dt2));
    }

    #[test]
    fn test_compare_dokument_identical() {
        // Test with completely identical documents
        let doc1 = create_test_dokument("00000000-0000-0000-0000-000000000001");
        let doc2 = create_test_dokument("00000000-0000-0000-0000-000000000001");
        assert!(compare_dokument(&doc1, &doc2));
    }

    #[test]
    fn test_compare_dokument_different_fields() {
        // Test with differences in each field
        let base_doc = create_test_dokument("00000000-0000-0000-0000-000000000001");

        // api_id
        let mut doc2 = base_doc.clone();
        doc2.api_id =
            Some(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap_or_default());
        assert!(!compare_dokument(&base_doc, &doc2));

        // drucksnr
        let mut doc2 = base_doc.clone();
        doc2.drucksnr = Some("Different Drucksnr".to_string());
        assert!(!compare_dokument(&base_doc, &doc2));

        // typ
        let mut doc2 = base_doc.clone();
        doc2.typ = models::Doktyp::Stellungnahme;
        assert!(!compare_dokument(&base_doc, &doc2));

        // titel
        let mut doc2 = base_doc.clone();
        doc2.titel = "Different Titel".to_string();
        assert!(!compare_dokument(&base_doc, &doc2));

        // kurztitel
        let mut doc2 = base_doc.clone();
        doc2.kurztitel = Some("Different Kurztitel".to_string());
        assert!(!compare_dokument(&base_doc, &doc2));

        // vorwort
        let mut doc2 = base_doc.clone();
        doc2.vorwort = Some("Different Vorwort".to_string());
        assert!(!compare_dokument(&base_doc, &doc2));

        // volltext
        let mut doc2 = base_doc.clone();
        doc2.volltext = "Different Volltext".to_string();
        assert!(!compare_dokument(&base_doc, &doc2));

        // zusammenfassung
        let mut doc2 = base_doc.clone();
        doc2.zusammenfassung = Some("Different Zusammenfassung".to_string());
        assert!(!compare_dokument(&base_doc, &doc2));

        // zp_modifiziert
        let mut doc2 = base_doc.clone();
        doc2.zp_modifiziert = base_doc.zp_modifiziert + chrono::Duration::hours(1);
        assert!(!compare_dokument(&base_doc, &doc2));

        // zp_referenz
        let mut doc2 = base_doc.clone();
        doc2.zp_referenz = base_doc.zp_referenz + chrono::Duration::hours(1);
        assert!(!compare_dokument(&base_doc, &doc2));

        // zp_erstellt
        let mut doc2 = base_doc.clone();
        doc2.zp_erstellt = Some(base_doc.zp_erstellt.unwrap() + chrono::Duration::hours(1));
        assert!(!compare_dokument(&base_doc, &doc2));

        // link
        let mut doc2 = base_doc.clone();
        doc2.link = "https://different.com".to_string();
        assert!(!compare_dokument(&base_doc, &doc2));

        // hash
        let mut doc2 = base_doc.clone();
        doc2.hash = "different-hash".to_string();
        assert!(!compare_dokument(&base_doc, &doc2));

        // meinung
        let mut doc2 = base_doc.clone();
        doc2.meinung = Some(5);
        assert!(!compare_dokument(&base_doc, &doc2));

        // schlagworte
        let mut doc2 = base_doc.clone();
        doc2.schlagworte = Some(vec!["Different1".to_string(), "Different2".to_string()]);
        assert!(!compare_dokument(&base_doc, &doc2));
    }

    #[test]
    fn test_compare_dokument_optional_fields() {
        // Test with one document having optional fields and the other not
        let mut doc1 = create_test_dokument("00000000-0000-0000-0000-000000000001");
        let mut doc2 = doc1.clone();

        // zp_erstellt: Some vs None
        doc2.zp_erstellt = None;
        assert!(!compare_dokument(&doc1, &doc2));

        // schlagworte: Some vs None
        doc1.zp_erstellt = None; // make them equal again
        doc2.schlagworte = None;
        assert!(!compare_dokument(&doc1, &doc2));

        // kurztitel: Some vs None
        doc1.schlagworte = None; // make them equal again
        doc2.kurztitel = None;
        assert!(!compare_dokument(&doc1, &doc2));
    }

    #[test]
    fn test_compare_dokument_dates() {
        // Test date fields with same milliseconds but different nanoseconds
        let mut doc1 = create_test_dokument("00000000-0000-0000-0000-000000000001");
        let mut doc2 = doc1.clone();

        // zp_modifiziert
        doc1.zp_modifiziert = create_test_datetime_with_nanos(100_000);
        doc2.zp_modifiziert = create_test_datetime_with_nanos(200_000);
        assert_eq!(
            doc1.zp_modifiziert.timestamp_millis(),
            doc2.zp_modifiziert.timestamp_millis()
        );
        assert!(compare_dokument(&doc1, &doc2));

        // zp_referenz
        doc1.zp_referenz = create_test_datetime_with_nanos(300_000);
        doc2.zp_referenz = create_test_datetime_with_nanos(400_000);
        assert_eq!(
            doc1.zp_referenz.timestamp_millis(),
            doc2.zp_referenz.timestamp_millis()
        );
        assert!(compare_dokument(&doc1, &doc2));

        // zp_erstellt
        doc1.zp_erstellt = Some(create_test_datetime_with_nanos(500_000));
        doc2.zp_erstellt = Some(create_test_datetime_with_nanos(600_000));
        assert_eq!(
            doc1.zp_erstellt.as_ref().unwrap().timestamp_millis(),
            doc2.zp_erstellt.as_ref().unwrap().timestamp_millis()
        );
        assert!(compare_dokument(&doc1, &doc2));
    }

    #[test]
    fn test_compare_dokument_autoren_different_order() {
        // Test with identical authors in different order
        let doc1 = create_test_dokument("00000000-0000-0000-0000-000000000001");
        let mut doc2 = doc1.clone();

        // Reverse the order of authors in doc2
        doc2.autoren = vec![create_test_autor("Person 2"), create_test_autor("Person 1")];

        assert!(compare_dokument(&doc1, &doc2));
    }

    #[test]
    fn test_compare_dokument_autoren_different() {
        // Test with different authors
        let doc1 = create_test_dokument("00000000-0000-0000-0000-000000000001");
        let mut doc2 = doc1.clone();

        // Change one author
        doc2.autoren = vec![
            create_test_autor("Person 1"),
            create_test_autor("Person 3"), // Different person
        ];

        assert!(!compare_dokument(&doc1, &doc2));

        // Different number of authors
        doc2.autoren = vec![
            create_test_autor("Person 1"),
            create_test_autor("Person 2"),
            create_test_autor("Person 3"),
        ];

        assert!(!compare_dokument(&doc1, &doc2));
    }

    #[test]
    fn test_compare_top_identical() {
        // Test with completely identical TOPs
        let top1 = create_test_top(1);
        let top2 = create_test_top(1);
        assert!(compare_top(&top1, &top2));
    }

    #[test]
    fn test_compare_top_different_fields() {
        // Test with differences in each field
        let base_top = create_test_top(1);

        // nummer
        let mut top2 = base_top.clone();
        top2.nummer = 2;
        assert!(!compare_top(&base_top, &top2));

        // titel
        let mut top2 = base_top.clone();
        top2.titel = "Different TOP".to_string();
        assert!(!compare_top(&base_top, &top2));

        // vorgang_id
        let mut top2 = base_top.clone();
        top2.vorgang_id = Some(vec![
            uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap_or_default(),
        ]);
        assert!(!compare_top(&base_top, &top2));
    }

    #[test]
    fn test_compare_top_dokumente_different_order() {
        // Test with identical documents in different order
        let top1 = create_test_top(1);
        let mut top2 = top1.clone();

        // Reverse the order of dokumente in top2
        if let Some(docs) = &mut top2.dokumente {
            docs.reverse();
        }

        assert!(compare_top(&top1, &top2));
    }

    #[test]
    fn test_compare_top_dokumente_different() {
        // Test with different documents
        let top1 = create_test_top(1);
        let mut top2 = top1.clone();

        // Change one document
        if let Some(docs) = &mut top2.dokumente {
            docs[0] = create_test_dokref_dokument("00000000-0000-0000-0000-000000000003");
        }

        assert!(!compare_top(&top1, &top2));

        // Different number of documents
        if let Some(docs) = &mut top2.dokumente {
            docs.push(create_test_dokref_dokument(
                "00000000-0000-0000-0000-000000000004",
            ));
        }

        assert!(!compare_top(&top1, &top2));
    }

    #[test]
    fn test_compare_top_dokumente_variants() {
        // Test with different DokRef variants (Dokument vs String)
        let top1 = create_test_top(1);
        let mut top2 = top1.clone();

        // Change one document to a different variant
        if let Some(docs) = &mut top2.dokumente {
            docs[0] = create_test_dokref_string("Test String");
        }

        assert!(!compare_top(&top1, &top2));
    }

    #[test]
    fn test_compare_top_optional_dokumente() {
        // Test with one TOP having Some dokumente and the other None
        let top1 = create_test_top(1);
        let mut top2 = top1.clone();

        top2.dokumente = None;

        assert!(!compare_top(&top1, &top2));
    }

    #[test]
    fn test_compare_sitzung_identical() {
        // Test with completely identical Sitzungen
        let sitz1 = create_test_sitzung("00000000-0000-0000-0000-000000000001");
        let sitz2 = create_test_sitzung("00000000-0000-0000-0000-000000000001");
        assert!(compare_sitzung(&sitz1, &sitz2));
    }

    #[test]
    fn test_compare_sitzung_different_fields() {
        // Test with differences in each field
        let base_sitz = create_test_sitzung("00000000-0000-0000-0000-000000000001");

        // api_id
        let mut sitz2 = base_sitz.clone();
        sitz2.api_id =
            Some(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap_or_default());
        assert!(!compare_sitzung(&base_sitz, &sitz2));

        // titel
        let mut sitz2 = base_sitz.clone();
        sitz2.titel = Some("Different Sitzung".to_string());
        assert!(!compare_sitzung(&base_sitz, &sitz2));

        // termin
        let mut sitz2 = base_sitz.clone();
        sitz2.termin = base_sitz.termin + Duration::hours(1);
        assert!(!compare_sitzung(&base_sitz, &sitz2));

        // gremium
        let mut sitz2 = base_sitz.clone();
        sitz2.gremium.name = "Different Gremium".to_string();
        assert!(!compare_sitzung(&base_sitz, &sitz2));

        // nummer
        let mut sitz2 = base_sitz.clone();
        sitz2.nummer = 43;
        assert!(!compare_sitzung(&base_sitz, &sitz2));

        // public
        let mut sitz2 = base_sitz.clone();
        sitz2.public = false;
        assert!(!compare_sitzung(&base_sitz, &sitz2));

        // link
        let mut sitz2 = base_sitz.clone();
        sitz2.link = Some("https://different.com".to_string());
        assert!(!compare_sitzung(&base_sitz, &sitz2));
    }

    #[test]
    fn test_compare_sitzung_tops_different_order() {
        // Test with identical TOPs in different order
        let sitz1 = create_test_sitzung("00000000-0000-0000-0000-000000000001");
        let mut sitz2 = sitz1.clone();

        // Reverse the order of TOPs
        sitz2.tops.reverse();

        assert!(compare_sitzung(&sitz1, &sitz2));
    }

    #[test]
    fn test_compare_sitzung_tops_different() {
        // Test with different TOPs
        let sitz1 = create_test_sitzung("00000000-0000-0000-0000-000000000001");
        let mut sitz2 = sitz1.clone();

        // Change one TOP
        sitz2.tops[0] = create_test_top(3);

        assert!(!compare_sitzung(&sitz1, &sitz2));

        // Different number of TOPs
        sitz2 = sitz1.clone();
        sitz2.tops.push(create_test_top(3));

        assert!(!compare_sitzung(&sitz1, &sitz2));
    }

    #[test]
    fn test_compare_sitzung_dokumente() {
        // Test with differences in dokumente
        let sitz1 = create_test_sitzung("00000000-0000-0000-0000-000000000001");
        let mut sitz2 = sitz1.clone();

        // Different dokument content
        if let Some(docs) = &mut sitz2.dokumente {
            docs[0] = create_test_dokument("00000000-0000-0000-0000-000000000005");
        }
        assert!(!compare_sitzung(&sitz1, &sitz2));

        // Different order (should still be equal)
        sitz2 = sitz1.clone();
        if let Some(docs) = &mut sitz2.dokumente {
            docs.reverse();
        }
        assert!(compare_sitzung(&sitz1, &sitz2));

        // Including optional dokumente (Some vs None)
        sitz2 = sitz1.clone();
        sitz2.dokumente = None;
        assert!(!compare_sitzung(&sitz1, &sitz2));
    }

    #[test]
    fn test_compare_sitzung_experten() {
        // Test with differences in experten
        let sitz1 = create_test_sitzung("00000000-0000-0000-0000-000000000001");
        let mut sitz2 = sitz1.clone();

        // Different experte content
        if let Some(experten) = &mut sitz2.experten {
            experten[0] = create_test_autor("Different Experte");
        }
        assert!(!compare_sitzung(&sitz1, &sitz2));

        // Different order (should still be equal)
        sitz2 = sitz1.clone();
        if let Some(experten) = &mut sitz2.experten {
            experten.reverse();
        }
        assert!(compare_sitzung(&sitz1, &sitz2));

        // Including optional experten (Some vs None)
        sitz2 = sitz1.clone();
        sitz2.experten = None;
        assert!(!compare_sitzung(&sitz1, &sitz2));
    }

    #[test]
    fn test_compare_vorgang_identical() {
        // Test with completely identical Vorgänge
        let vg1 = create_test_vorgang("00000000-0000-0000-0000-000000000001");
        let vg2 = create_test_vorgang("00000000-0000-0000-0000-000000000001");
        assert!(compare_vorgang(&vg1, &vg2));
    }

    #[test]
    fn test_compare_vorgang_different_fields() {
        // Test with differences in each basic field
        let base_vg = create_test_vorgang("00000000-0000-0000-0000-000000000001");

        // api_id
        let mut vg2 = base_vg.clone();
        vg2.api_id =
            uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap_or_default();
        assert!(!compare_vorgang(&base_vg, &vg2));

        // titel
        let mut vg2 = base_vg.clone();
        vg2.titel = "Different Vorgang".to_string();
        assert!(!compare_vorgang(&base_vg, &vg2));

        // kurztitel
        let mut vg2 = base_vg.clone();
        vg2.kurztitel = Some("Different Kurztitel".to_string());
        assert!(!compare_vorgang(&base_vg, &vg2));

        // wahlperiode
        let mut vg2 = base_vg.clone();
        vg2.wahlperiode = 20;
        assert!(!compare_vorgang(&base_vg, &vg2));

        // verfassungsaendernd
        let mut vg2 = base_vg.clone();
        vg2.verfassungsaendernd = true;
        assert!(!compare_vorgang(&base_vg, &vg2));

        // typ
        let mut vg2 = base_vg.clone();
        vg2.typ = models::Vorgangstyp::GgLandVolk;
        assert!(!compare_vorgang(&base_vg, &vg2));
    }

    #[test]
    fn test_compare_vorgang_optional_fields() {
        // Test with differences in optional fields (ids, links)
        let vg1 = create_test_vorgang("00000000-0000-0000-0000-000000000001");
        let mut vg2 = vg1.clone();

        // ids
        vg2.ids = Some(vec![models::VgIdent {
            id: "Different ID".to_string(),
            typ: models::VgIdentTyp::Vorgnr,
        }]);
        assert!(!compare_vorgang(&vg1, &vg2));

        // links
        vg2 = vg1.clone();
        vg2.links = Some(vec!["https://different.com".to_string()]);
        assert!(!compare_vorgang(&vg1, &vg2));

        // Some vs None
        vg2 = vg1.clone();
        vg2.links = None;
        assert!(!compare_vorgang(&vg1, &vg2));
    }

    #[test]
    fn test_compare_vorgang_initiatoren() {
        // Test with differences in initiatoren
        let vg1 = create_test_vorgang("00000000-0000-0000-0000-000000000001");
        let mut vg2 = vg1.clone();

        // Different initiator content
        vg2.initiatoren[0] = create_test_autor("Different Initiator");
        assert!(!compare_vorgang(&vg1, &vg2));

        // Different order (should still be equal)
        vg2 = vg1.clone();
        vg2.initiatoren.reverse();
        assert!(compare_vorgang(&vg1, &vg2));

        // Different number of initiatoren
        vg2 = vg1.clone();
        vg2.initiatoren.push(create_test_autor("New Initiator"));
        assert!(!compare_vorgang(&vg1, &vg2));
    }

    #[test]
    fn test_compare_vorgang_stationen_different_order() {
        // Test with identical stationen in different order
        let vg1 = create_test_vorgang("00000000-0000-0000-0000-000000000001");
        let mut vg2 = vg1.clone();

        // Reverse the order of stationen
        vg2.stationen.reverse();

        assert!(compare_vorgang(&vg1, &vg2));
    }

    #[test]
    fn test_compare_vorgang_stationen_different() {
        // Test with different stationen
        let vg1 = create_test_vorgang("00000000-0000-0000-0000-000000000001");
        let mut vg2 = vg1.clone();

        // Change one station
        vg2.stationen[0] = create_test_station("00000000-0000-0000-0000-000000000003");

        assert!(!compare_vorgang(&vg1, &vg2));

        // Different number of stationen
        vg2 = vg1.clone();
        vg2.stationen
            .push(create_test_station("00000000-0000-0000-0000-000000000003"));

        assert!(!compare_vorgang(&vg1, &vg2));
    }

    #[test]
    fn test_compare_vorgang_stationen_dates() {
        // Test date fields with same milliseconds but different nanoseconds
        let mut vg1 = create_test_vorgang("00000000-0000-0000-0000-000000000001");
        let mut vg2 = vg1.clone();

        // zp_start
        vg1.stationen[0].zp_start = create_test_datetime_with_nanos(100_000);
        vg2.stationen[0].zp_start = create_test_datetime_with_nanos(200_000);
        assert_eq!(
            vg1.stationen[0].zp_start.timestamp_millis(),
            vg2.stationen[0].zp_start.timestamp_millis()
        );
        assert!(compare_vorgang(&vg1, &vg2));

        // zp_modifiziert
        if let (Some(mod1), Some(mod2)) = (
            &mut vg1.stationen[0].zp_modifiziert,
            &mut vg2.stationen[0].zp_modifiziert,
        ) {
            *mod1 = create_test_datetime_with_nanos(300_000);
            *mod2 = create_test_datetime_with_nanos(400_000);
            assert_eq!(mod1.timestamp_millis(), mod2.timestamp_millis());
        }
        assert!(compare_vorgang(&vg1, &vg2));
    }

    #[test]
    fn test_compare_vorgang_station_dokumente() {
        // Test with differences in station dokumente
        let vg1 = create_test_vorgang("00000000-0000-0000-0000-000000000001");
        let mut vg2 = vg1.clone();

        // Different dokument content
        vg2.stationen[0].dokumente[0] =
            create_test_dokref_dokument("00000000-0000-0000-0000-000000000003");
        assert!(!compare_vorgang(&vg1, &vg2));

        // Different order (should still be equal)
        vg2 = vg1.clone();
        vg2.stationen[0].dokumente.reverse();
        assert!(compare_vorgang(&vg1, &vg2));

        // Different variants
        vg2 = vg1.clone();
        vg2.stationen[0].dokumente[0] = create_test_dokref_string("Test String");
        assert!(!compare_vorgang(&vg1, &vg2));

        // Different number of documents
        vg2 = vg1.clone();
        vg2.stationen[0].dokumente.push(create_test_dokref_dokument(
            "00000000-0000-0000-0000-000000000003",
        ));
        assert!(!compare_vorgang(&vg1, &vg2));
    }

    #[test]
    fn test_compare_vorgang_station_stellungnahmen() {
        // Test with differences in station stellungnahmen
        let mut vg1 = create_test_vorgang("00000000-0000-0000-0000-000000000001");
        let mut vg2 = vg1.clone();

        // Different stellungnahme content
        if let (Some(_), Some(st2)) = (
            &mut vg1.stationen[0].stellungnahmen,
            &mut vg2.stationen[0].stellungnahmen,
        ) {
            st2[0] = create_test_dokument("00000000-0000-0000-0000-000000000007");
        }
        assert!(!compare_vorgang(&vg1, &vg2));

        // Different order (should still be equal)
        vg2 = vg1.clone();
        if let Some(st) = &mut vg2.stationen[0].stellungnahmen {
            st.reverse();
        }
        assert!(compare_vorgang(&vg1, &vg2));

        // Some vs None
        vg2 = vg1.clone();
        vg2.stationen[0].stellungnahmen = None;
        assert!(!compare_vorgang(&vg1, &vg2));
    }

    // Helper functions for creating test objects
    fn create_test_datetime() -> DateTime<Utc> {
        let dt = Utc::now();
        dt.date_naive()
            .and_hms_nano_opt(16, 32, 32, 0)
            .unwrap_or_else(|| dt.naive_utc())
            .and_utc()
    }

    fn create_test_datetime_with_nanos(nanos: u32) -> DateTime<Utc> {
        let dt = Utc::now();
        dt.date_naive()
            .and_hms_nano_opt(dt.hour(), dt.minute(), dt.second(), nanos)
            .unwrap_or_else(|| dt.naive_utc())
            .and_utc()
    }

    fn create_test_autor(person: &str) -> Autor {
        Autor {
            person: Some(person.to_string()),
            organisation: "Test Organisation".to_string(),
            fachgebiet: Some("Test Fachgebiet".to_string()),
            lobbyregister: Some("Test Lobbyregister".to_string()),
        }
    }

    fn create_test_dokument(api_id: &str) -> Dokument {
        Dokument {
            api_id: Some(uuid::Uuid::parse_str(api_id).unwrap_or_default()),
            drucksnr: Some("Test Drucksnr".to_string()),
            typ: models::Doktyp::Entwurf,
            titel: "Test Titel".to_string(),
            dc_type: default::Default::default(),
            touched_by: None,
            kurztitel: Some("Test Kurztitel".to_string()),
            vorwort: Some("Test Vorwort".to_string()),
            volltext: "Test Volltext".to_string(),
            zusammenfassung: Some("Test Zusammenfassung".to_string()),
            zp_modifiziert: create_test_datetime(),
            zp_referenz: create_test_datetime(),
            zp_erstellt: Some(create_test_datetime()),
            link: "https://test.com".to_string(),
            hash: "test-hash".to_string(),
            meinung: Some(3),
            schlagworte: Some(vec!["Test1".to_string(), "Test2".to_string()]),
            autoren: vec![create_test_autor("Person 1"), create_test_autor("Person 2")],
        }
    }

    fn create_test_dokref_dokument(api_id: &str) -> DokRef {
        DokRef::Dokument(Box::new(create_test_dokument(api_id)))
    }

    fn create_test_dokref_string(s: &str) -> DokRef {
        DokRef::StringRef(Box::new(StringRef { dc_type: default::Default::default(), value: s.to_string() }))
    }

    fn create_test_top(nummer: i32) -> Top {
        Top {
            nummer: nummer as u32,
            titel: "Test TOP".to_string(),
            vorgang_id: Some(vec![
                uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap_or_default(),
            ]),
            dokumente: Some(vec![
                create_test_dokref_dokument("00000000-0000-0000-0000-000000000001"),
                create_test_dokref_dokument("00000000-0000-0000-0000-000000000002"),
            ]),
        }
    }

    fn create_test_sitzung(api_id: &str) -> Sitzung {
        Sitzung {
            api_id: Some(uuid::Uuid::parse_str(api_id).unwrap_or_default()),
            titel: Some("Test Sitzung".to_string()),
            termin: create_test_datetime(),
            touched_by: None,
            gremium: models::Gremium {
                parlament: models::Parlament::Bt,
                wahlperiode: 19,
                name: "Test Gremium".to_string(),
                link: None,
            },
            nummer: 42,
            public: true,
            link: Some("https://test.com".to_string()),
            tops: vec![create_test_top(1), create_test_top(2)],
            dokumente: Some(vec![
                create_test_dokument("00000000-0000-0000-0000-000000000003"),
                create_test_dokument("00000000-0000-0000-0000-000000000004"),
            ]),
            experten: Some(vec![
                create_test_autor("Experte 1"),
                create_test_autor("Experte 2"),
            ]),
        }
    }

    fn create_test_station(api_id: &str) -> Station {
        Station {
            api_id: Some(uuid::Uuid::parse_str(api_id).unwrap_or_default()),
            titel: Some("Test Station".to_string()),
            zp_start: create_test_datetime(),
            touched_by: None,
            zp_modifiziert: Some(create_test_datetime()),
            gremium: Some(models::Gremium {
                parlament: models::Parlament::Bt,
                wahlperiode: 19,
                name: "Test Gremium".to_string(),
                link: None,
            }),
            gremium_federf: Some(true),
            link: Some("https://test.com".to_string()),
            parlament: models::Parlament::Bt,
            typ: models::Stationstyp::ParlInitiativ,
            trojanergefahr: Some(5),
            schlagworte: Some(vec!["Test1".to_string(), "Test2".to_string()]),
            additional_links: Some(vec![
                "https://test1.com".to_string(),
                "https://test2.com".to_string(),
            ]),
            dokumente: vec![
                create_test_dokref_dokument("00000000-0000-0000-0000-000000000001"),
                create_test_dokref_dokument("00000000-0000-0000-0000-000000000002"),
            ],
            stellungnahmen: Some(vec![
                create_test_dokument("00000000-0000-0000-0000-000000000005"),
                create_test_dokument("00000000-0000-0000-0000-000000000006"),
            ]),
        }
    }

    fn create_test_vorgang(api_id: &str) -> Vorgang {
        Vorgang {
            api_id: uuid::Uuid::parse_str(api_id).unwrap_or_default(),
            titel: "Test Vorgang".to_string(),
            kurztitel: Some("Test Kurztitel".to_string()),
            wahlperiode: 19,
            lobbyregister: todo!("Implement comparison for Lobbyregister"),
            touched_by: None,
            verfassungsaendernd: false,
            typ: models::Vorgangstyp::GgEinspruch,
            ids: Some(vec![
                models::VgIdent {
                    id: "ID1".to_string(),
                    typ: models::VgIdentTyp::Vorgnr,
                },
                models::VgIdent {
                    id: "ID2".to_string(),
                    typ: models::VgIdentTyp::Initdrucks,
                },
            ]),
            links: Some(vec![
                "https://test1.com".to_string(),
                "https://test2.com".to_string(),
            ]),
            initiatoren: vec![
                create_test_autor("Initiator 1"),
                create_test_autor("Initiator 2"),
            ],
            stationen: vec![
                create_test_station("00000000-0000-0000-0000-000000000001"),
                create_test_station("00000000-0000-0000-0000-000000000002"),
            ],
        }
    }
}
