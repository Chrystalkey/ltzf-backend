use tokio::signal;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub mod notify;

pub async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
    _ = ctrl_c => {},
    _ = terminate => {},
    }
}
pub fn as_option<T>(v: Vec<T>) -> Option<Vec<T>> {
    if v.is_empty() { None } else { Some(v) }
}
// Function to initialize tracing for logging
pub fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "RUST_LOG=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

#[cfg(test)]
pub(crate) mod test {
    use sha256::digest;

    use crate::{Configuration, LTZFServer, Result};
    pub const MASTER_URL: &str = "postgres://ltzf-user:ltzf-pass@localhost:5432/ltzf";
    pub(crate) struct TestSetup {
        pub(crate) name: &'static str,
        pub(crate) server: LTZFServer,
    }
    impl TestSetup {
        pub(crate) async fn new(name: &'static str) -> Self {
            return Self {
                name,
                server: setup_server(name).await.unwrap(),
            };
        }
        pub(crate) async fn teardown(&self) {
            cleanup_server(self.name).await.unwrap();
        }
    }
    async fn setup_server(dbname: &str) -> Result<LTZFServer> {
        let create_pool = sqlx::PgPool::connect(MASTER_URL).await.unwrap();
        sqlx::query(&format!("DROP DATABASE IF EXISTS {} WITH (FORCE);", dbname))
            .execute(&create_pool)
            .await?;
        sqlx::query(&format!(
            "CREATE DATABASE {} WITH OWNER 'ltzf-user'",
            dbname
        ))
        .execute(&create_pool)
        .await?;
        let pool = sqlx::PgPool::connect(&format!(
            "postgres://ltzf-user:ltzf-pass@localhost:5432/{}",
            dbname
        ))
        .await
        .unwrap();
        sqlx::migrate!().run(&pool).await?;
        let hash = digest("total-nutzloser-wert");
        sqlx::query!(
            "INSERT INTO api_keys(key_hash, scope, created_by)
            VALUES
            ($1, (SELECT id FROM api_scope WHERE value = 'keyadder' LIMIT 1), (SELECT last_value FROM api_keys_id_seq))
            ON CONFLICT DO NOTHING;", hash)
        .execute(&pool).await?;
        Ok(LTZFServer::new(pool, Configuration::default(), None))
    }

    async fn cleanup_server(dbname: &str) -> Result<()> {
        let create_pool = sqlx::PgPool::connect(MASTER_URL).await.unwrap();
        sqlx::query(&format!("DROP DATABASE {} WITH (FORCE);", dbname))
            .execute(&create_pool)
            .await?;
        Ok(())
    }

    pub(crate) mod generate {
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
        pub(crate) fn alternate_station() -> models::Station {
            let stat = default_station();
            models::Station {
                api_id: Some(Uuid::from_str("b18bde64-c0ff-eeee-ff0c-deadbeefaeea").unwrap()),
                typ: models::Stationstyp::ParlAblehnung,
                dokumente: vec![],
                zp_start: chrono::DateTime::parse_from_rfc3339("1951-01-01T22:01:02+00:00")
                    .unwrap()
                    .to_utc(),
                zp_modifiziert: Some(
                    chrono::DateTime::parse_from_rfc3339("1951-01-02T22:01:02+00:00")
                        .unwrap()
                        .to_utc(),
                ),
                ..stat
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
}
