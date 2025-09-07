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

pub(crate) mod auth {
    use rand::distr::Alphanumeric;
    use rand::{Rng, rng};
    use sha256::digest;
    pub(crate) fn keytag_of(thing: &str) -> String {
        thing.chars().take(16).collect()
    }
    pub(crate) fn hash_full_key(salt: &str, full_key: &str) -> String {
        hash_secret(salt, &full_key.chars().skip(16).collect::<String>())
    }
    pub(crate) fn hash_secret(salt: &str, secret: &str) -> String {
        digest(salt.chars().chain(secret.chars()).collect::<String>())
    }

    pub fn generate_api_key() -> String {
        let key: String = "ltzf_"
            .chars()
            .chain(
                rng()
                    .sample_iter(&Alphanumeric)
                    .take(59)
                    .map(char::from)
                    .map(|c| {
                        if rng().random_bool(0.5f64) {
                            c.to_ascii_lowercase()
                        } else {
                            c.to_ascii_uppercase()
                        }
                    }),
            )
            .collect();
        key
    }
    pub(crate) fn generate_salt() -> String {
        rng()
            .sample_iter(&Alphanumeric)
            .take(16)
            .map(char::from)
            .map(|c| {
                if rng().random_bool(0.5f64) {
                    c.to_ascii_lowercase()
                } else {
                    c.to_ascii_uppercase()
                }
            })
            .collect()
    }
    pub(crate) async fn find_new_key(
        tx: &mut sqlx::PgTransaction<'_>,
    ) -> crate::Result<(String, String)> {
        let mut new_key = crate::utils::auth::generate_api_key();
        let mut new_salt = crate::utils::auth::generate_salt();

        loop {
            let found = sqlx::query!("SELECT id FROM api_keys")
                .fetch_optional(&mut **tx)
                .await?;
            if found.is_some() {
                return Ok((new_key, new_salt));
            } else {
                new_key = crate::utils::auth::generate_api_key();
                new_salt = crate::utils::auth::generate_salt();
            }
        }
    }
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
        sqlx::query(&format!("DROP DATABASE IF EXISTS {dbname} WITH (FORCE);"))
            .execute(&create_pool)
            .await?;
        sqlx::query(&format!("CREATE DATABASE {dbname} WITH OWNER 'ltzf-user'"))
            .execute(&create_pool)
            .await?;
        let pool = sqlx::PgPool::connect(&format!(
            "postgres://ltzf-user:ltzf-pass@localhost:5432/{dbname}"
        ))
        .await
        .unwrap();
        sqlx::migrate!().run(&pool).await?;
        let hash = digest("total-nutzloser-wert");
        sqlx::query!(
            "INSERT INTO api_keys(key_hash, scope, created_by, salt, keytag)
            VALUES
            ($1, (SELECT id FROM api_scope WHERE value = 'keyadder' LIMIT 1), (SELECT last_value FROM api_keys_id_seq), $2, $3)
            ON CONFLICT DO NOTHING;", hash, "salziger-salzkeks", "total-nutzlos")
        .execute(&pool).await?;
        Ok(LTZFServer::new(
            pool,
            Configuration {
                per_object_scraper_log_size: 5,
                ..Default::default()
            },
            None,
        ))
    }

    async fn cleanup_server(dbname: &str) -> Result<()> {
        let create_pool = sqlx::PgPool::connect(MASTER_URL).await.unwrap();
        sqlx::query(&format!("DROP DATABASE {dbname} WITH (FORCE);"))
            .execute(&create_pool)
            .await?;
        Ok(())
    }

    #[allow(unused)]
    pub(crate) mod generate {
        use std::str::FromStr;

        use crate::LTZFServer;
        use crate::Result;
        use openapi::models;
        use sha256::digest;
        use uuid::Uuid;

        pub(crate) async fn setup_server(name: &str) -> Result<LTZFServer> {
            let dburl = std::env::var("DATABASE_URL")
                .expect("Expected to find working DATABASE_URL for testing");
            let config = crate::Configuration {
                mail_server: None,
                mail_user: None,
                mail_password: None,
                mail_sender: None,
                per_object_scraper_log_size: 5,
                req_limit_count: 4096,
                req_limit_interval: 2,
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
            let dropquery = format!("DROP DATABASE IF EXISTS \"testing_{}\" WITH (FORCE);", name);
            let query = format!(
                "CREATE DATABASE \"testing_{}\" WITH OWNER 'ltzf-user';",
                name
            );
            sqlx::query(&dropquery)
                .execute(&master_server.sqlx_db)
                .await?;
            sqlx::query(&query).execute(&master_server.sqlx_db).await?;

            let db_url = config
                .db_url
                .replace("5432/ltzf", &format!("5432/testing_{}", name));
            let oconfig = crate::Configuration {
                db_url: db_url.clone(),
                per_object_scraper_log_size: 5,
                ..config
            };
            let out_server = LTZFServer {
                config: oconfig,
                mailbundle: None,
                sqlx_db: sqlx::postgres::PgPool::connect(&db_url).await?,
            };
            sqlx::migrate!().run(&out_server.sqlx_db).await?;

            // insert api key
            let keyadder_hash = digest(out_server.config.keyadder_key.as_str());
            sqlx::query!(
                "INSERT INTO api_keys(key_hash, scope, created_by, salt, keytag)
                VALUES
                ($1, (SELECT id FROM api_scope WHERE value = 'keyadder' LIMIT 1), (SELECT last_value FROM api_keys_id_seq), $2, $3)
                ON CONFLICT DO NOTHING;", keyadder_hash, "salt-and-curry-no-pepper", "tegernsee-apfels")
            .execute(&out_server.sqlx_db).await?;

            Ok(out_server)
        }
        pub(crate) mod random_helpers {
            use chrono::DateTime;
            use chrono::Utc;
            use openapi::models;
            use rand::rngs::StdRng;
            use rand::{Rng, SeedableRng};
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            use uuid::Uuid;
            // Helper function to generate random strings
            pub(crate) fn random_string(
                rng: &mut StdRng,
                prefix: &str,
                min_len: usize,
                max_len: usize,
            ) -> String {
                let len = rng.random_range(min_len..=max_len);
                let mut result = prefix.to_string();
                for _ in 0..len {
                    result.push(rng.random_range('a'..='z'));
                }
                result
            }

            // Helper function to randomly choose from enum variants
            pub(crate) fn random_enum<T: Clone>(rng: &mut StdRng, variants: &[T]) -> T {
                variants[rng.random_range(0..variants.len())].clone()
            }

            // Helper function to generate random UUIDs
            pub(crate) fn random_uuid(rng: &mut StdRng) -> uuid::Uuid {
                let mut bytes = [0u8; 16];
                rng.fill(&mut bytes);
                uuid::Uuid::from_bytes(bytes)
            }

            // Helper function to generate random dates
            pub(crate) fn random_date(rng: &mut StdRng) -> chrono::DateTime<chrono::Utc> {
                let year = rng.random_range(2020..=2030);
                let month = rng.random_range(1..=12);
                let day = rng.random_range(1..=28);
                let hour = rng.random_range(0..=23);
                let minute = rng.random_range(0..=59);
                let second = rng.random_range(0..=59);

                chrono::DateTime::parse_from_rfc3339(&format!(
                    "{}-{:02}-{:02}T{:02}:{:02}:{:02}+00:00",
                    year, month, day, hour, minute, second
                ))
                .unwrap()
                .to_utc()
            }

            // Helper function to generate random authors
            pub(crate) fn random_autor(rng: &mut StdRng) -> models::Autor {
                let has_person = rng.random_bool(0.7);
                let has_fachgebiet = rng.random_bool(0.3);
                let has_lobbyregister = rng.random_bool(0.2);

                models::Autor {
                    person: if has_person {
                        Some(random_string(rng, "Person_", 5, 15))
                    } else {
                        None
                    },
                    organisation: random_string(rng, "Org_", 8, 25),
                    fachgebiet: if has_fachgebiet {
                        Some(random_string(rng, "Fach_", 5, 20))
                    } else {
                        None
                    },
                    lobbyregister: if has_lobbyregister {
                        Some(format!(
                            "https://lobby.{}.de/{}",
                            random_string(rng, "", 3, 8),
                            random_string(rng, "", 8, 16)
                        ))
                    } else {
                        None
                    },
                }
            }
            pub(crate) fn random_stellungnahme(rnd: &mut StdRng) -> models::Dokument {
                random_base_dokument(rnd, true)
            }
            pub(crate) fn random_dokument(rnd: &mut StdRng) -> models::Dokument {
                random_base_dokument(rnd, false)
            }
            // Helper function to generate random documents
            pub(crate) fn random_base_dokument(
                rng: &mut StdRng,
                is_stln: bool,
            ) -> models::Dokument {
                let doktyp_variants = [
                    models::Doktyp::Entwurf,
                    models::Doktyp::Antrag,
                    models::Doktyp::Anfrage,
                    models::Doktyp::Antwort,
                    models::Doktyp::Mitteilung,
                    models::Doktyp::Gutachten,
                ];

                let has_drucksnr = rng.random_bool(0.8);
                let has_kurztitel = rng.random_bool(0.6);
                let has_vorwort = rng.random_bool(0.4);
                let has_zusammenfassung = rng.random_bool(0.5);
                let has_schlagworte = rng.random_bool(0.7);
                let has_zp_erstellt = rng.random_bool(0.6);

                let num_autoren = rng.random_range(1..=3);
                let mut autoren = Vec::new();
                for _ in 0..num_autoren {
                    autoren.push(random_autor(rng));
                }

                let schlagworte_count = if has_schlagworte {
                    rng.random_range(1..=5)
                } else {
                    0
                };
                let schlagworte = if has_schlagworte {
                    let mut words = Vec::new();
                    for _ in 0..schlagworte_count {
                        words.push(random_string(rng, "tag_", 3, 12));
                    }
                    Some(words)
                } else {
                    None
                };

                models::Dokument {
                    api_id: Some(random_uuid(rng)),
                    touched_by: None,
                    drucksnr: if has_drucksnr {
                        Some(format!(
                            "{}/{}",
                            rng.random_range(15..=25),
                            rng.random_range(100..=999)
                        ))
                    } else {
                        None
                    },
                    typ: if is_stln {
                        models::Doktyp::Stellungnahme
                    } else {
                        random_enum(rng, &doktyp_variants)
                    },
                    titel: random_string(rng, "Titel für ", 20, 80),
                    kurztitel: if has_kurztitel {
                        Some(random_string(rng, "Kurz: ", 10, 40))
                    } else {
                        None
                    },
                    vorwort: if has_vorwort && !is_stln {
                        Some(random_string(rng, "Vorwort: ", 30, 100))
                    } else {
                        None
                    },
                    volltext: random_string(rng, "Volltext des Dokuments: ", 100, 500),
                    zusammenfassung: if has_zusammenfassung {
                        Some(random_string(rng, "Zusammenfassung: ", 50, 200))
                    } else {
                        None
                    },
                    zp_modifiziert: random_date(rng),
                    zp_referenz: random_date(rng),
                    zp_erstellt: if has_zp_erstellt {
                        Some(random_date(rng))
                    } else {
                        None
                    },
                    link: format!(
                        "https://{}.de/dokument/{}",
                        random_string(rng, "", 5, 10),
                        random_string(rng, "", 8, 16)
                    ),
                    hash: random_string(rng, "", 20, 40),
                    meinung: if is_stln {
                        Some(rng.random_range(1..=5))
                    } else {
                        None
                    },
                    schlagworte,
                    autoren,
                }
            }

            // Helper function to generate random stations
            pub(crate) fn random_station(rng: &mut StdRng) -> models::Station {
                let stationstyp_variants = [
                    models::Stationstyp::ParlAusschber,
                    models::Stationstyp::ParlVollvlsgn,
                    models::Stationstyp::ParlAkzeptanz,
                    models::Stationstyp::ParlAblehnung,
                    models::Stationstyp::ParlZurueckgz,
                    models::Stationstyp::ParlGgentwurf,
                ];

                let parlament_variants = [
                    models::Parlament::Bt,
                    models::Parlament::Br,
                    models::Parlament::Bv,
                    models::Parlament::Bb,
                    models::Parlament::By,
                    models::Parlament::Be,
                ];

                let has_titel = rng.random_bool(0.6);
                let has_zp_modifiziert = rng.random_bool(0.7);
                let has_link = rng.random_bool(0.8);
                let has_gremium_federf = rng.random_bool(0.4);
                let has_trojanergefahr = rng.random_bool(0.5);
                let has_schlagworte = rng.random_bool(0.6);
                let has_additional_links = rng.random_bool(0.3);
                let has_stellungnahmen = rng.random_bool(0.4);

                let num_dokumente = rng.random_range(1..=4);
                let mut dokumente = Vec::new();
                for _ in 0..num_dokumente {
                    dokumente.push(models::StationDokumenteInner::Dokument(Box::new(
                        random_dokument(rng),
                    )));
                }

                let schlagworte_count = if has_schlagworte {
                    rng.random_range(1..=4)
                } else {
                    0
                };
                let schlagworte = if has_schlagworte {
                    let mut words = Vec::new();
                    for _ in 0..schlagworte_count {
                        words.push(random_string(rng, "station_", 5, 15));
                    }
                    Some(words)
                } else {
                    None
                };

                let additional_links_count = if has_additional_links {
                    rng.random_range(1..=3)
                } else {
                    0
                };
                let additional_links = if has_additional_links {
                    let mut links = Vec::new();
                    for _ in 0..additional_links_count {
                        links.push(format!(
                            "https://{}.de/link/{}",
                            random_string(rng, "", 5, 10),
                            random_string(rng, "", 8, 16)
                        ));
                    }
                    Some(links)
                } else {
                    None
                };

                let stellungnahmen_count = if has_stellungnahmen {
                    rng.random_range(1..=2)
                } else {
                    0
                };
                let stellungnahmen = if has_stellungnahmen {
                    let mut stn = Vec::new();
                    for _ in 0..stellungnahmen_count {
                        stn.push(models::StationDokumenteInner::Dokument(Box::new(
                            random_stellungnahme(rng),
                        )));
                    }
                    Some(stn)
                } else {
                    None
                };

                models::Station {
                    api_id: Some(random_uuid(rng)),
                    touched_by: None,
                    titel: if has_titel {
                        Some(random_string(rng, "Station: ", 15, 50))
                    } else {
                        None
                    },
                    zp_start: random_date(rng),
                    zp_modifiziert: if has_zp_modifiziert {
                        Some(random_date(rng))
                    } else {
                        None
                    },
                    gremium: models::Gremium {
                        parlament: random_enum(rng, &parlament_variants),
                        wahlperiode: rng.random_range(15..=25),
                        link: if rng.random_bool(0.7) {
                            Some(format!(
                                "https://{}.de/gremium",
                                random_string(rng, "", 5, 10)
                            ))
                        } else {
                            None
                        },
                        name: random_string(rng, "Ausschuss für ", 20, 40),
                    },
                    gremium_federf: if has_gremium_federf {
                        Some(rng.random_bool(0.5))
                    } else {
                        None
                    },
                    link: if has_link {
                        Some(format!(
                            "https://{}.de/station/{}",
                            random_string(rng, "", 5, 10),
                            random_string(rng, "", 8, 16)
                        ))
                    } else {
                        None
                    },
                    typ: random_enum(rng, &stationstyp_variants),
                    trojanergefahr: if has_trojanergefahr {
                        Some(rng.random_range(1..=10))
                    } else {
                        None
                    },
                    schlagworte,
                    dokumente,
                    additional_links,
                    stellungnahmen,
                }
            }
        }

        pub(crate) fn vorgang_with_seed(seed: u64) -> models::Vorgang {
            use rand::rngs::StdRng;
            use rand::{Rng, SeedableRng};
            use random_helpers::*;
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            // Create a deterministic RNG from the seed
            let mut rng = StdRng::seed_from_u64(seed);

            // Generate the main vorgang
            let vorgangstyp_variants = [
                models::Vorgangstyp::GgZustimmung,
                models::Vorgangstyp::GgEinspruch,
                models::Vorgangstyp::GgLandParl,
                models::Vorgangstyp::GgLandVolk,
                models::Vorgangstyp::BwEinsatz,
                models::Vorgangstyp::Sonstig,
            ];

            let vgident_typ_variants = [
                models::VgIdentTyp::Initdrucks,
                models::VgIdentTyp::Vorgnr,
                models::VgIdentTyp::ApiId,
                models::VgIdentTyp::Sonstig,
            ];

            let has_kurztitel = rng.random_bool(0.6);
            let has_links = rng.random_bool(0.7);
            let has_ids = rng.random_bool(0.8);
            let has_lobbyregister = rng.random_bool(0.4);
            let has_touched_by = rng.random_bool(0.3);

            let num_initiatoren = rng.random_range(1..=4);
            let mut initiatoren = Vec::new();
            for _ in 0..num_initiatoren {
                initiatoren.push(random_autor(&mut rng));
            }

            let num_stationen = rng.random_range(1..=5);
            let mut stationen = Vec::new();
            for _ in 0..num_stationen {
                stationen.push(random_station(&mut rng));
            }

            let links_count = if has_links {
                rng.random_range(1..=3)
            } else {
                0
            };
            let links = if has_links {
                let mut link_list = Vec::new();
                for _ in 0..links_count {
                    link_list.push(format!(
                        "https://{}.de/vorgang/{}",
                        random_string(&mut rng, "", 5, 10),
                        random_string(&mut rng, "", 8, 16)
                    ));
                }
                Some(link_list)
            } else {
                None
            };

            let ids_count = if has_ids { rng.random_range(1..=3) } else { 0 };
            let ids = if has_ids {
                let mut id_list = Vec::new();
                for _ in 0..ids_count {
                    id_list.push(models::VgIdent {
                        id: random_string(&mut rng, "id_", 5, 15),
                        typ: random_enum(&mut rng, &vgident_typ_variants),
                    });
                }
                Some(id_list)
            } else {
                None
            };

            let lobbyregister_count = if has_lobbyregister {
                rng.random_range(1..=2)
            } else {
                0
            };
            let lobbyregister = if has_lobbyregister {
                let mut lobby_list = Vec::new();
                for _ in 0..lobbyregister_count {
                    let betroffene_count = rng.random_range(1..=3);
                    let mut betroffene = Vec::new();
                    for _ in 0..betroffene_count {
                        betroffene.push(format!(
                            "{}/{}",
                            rng.random_range(15..=25),
                            rng.random_range(100..=999)
                        ));
                    }

                    lobby_list.push(models::Lobbyregeintrag {
                        organisation: random_autor(&mut rng),
                        interne_id: random_string(&mut rng, "lobby_", 8, 20),
                        intention: random_string(&mut rng, "Intention: ", 20, 60),
                        link: format!(
                            "https://lobby.{}.de/{}",
                            random_string(&mut rng, "", 5, 10),
                            random_string(&mut rng, "", 8, 16)
                        ),
                        betroffene_drucksachen: betroffene,
                    });
                }
                Some(lobby_list)
            } else {
                None
            };

            models::Vorgang {
                api_id: random_uuid(&mut rng),
                touched_by: if has_touched_by {
                    Some(vec![models::TouchedByInner {
                        scraper_id: Some(random_uuid(&mut rng)),
                        key: Some(digest(random_string(&mut rng, "key_", 10, 30))),
                    }])
                } else {
                    None
                },
                titel: random_string(&mut rng, "Vorgang: ", 20, 80),
                kurztitel: if has_kurztitel {
                    Some(random_string(&mut rng, "Kurz: ", 10, 40))
                } else {
                    None
                },
                wahlperiode: rng.random_range(15..=25),
                verfassungsaendernd: rng.random_bool(0.2),
                typ: random_enum(&mut rng, &vorgangstyp_variants),
                ids,
                links,
                initiatoren,
                stationen,
                lobbyregister,
            }
        }
        pub(crate) fn station_with_seed(seed: u64) -> models::Station {
            use rand::rngs::StdRng;
            use rand::{Rng, SeedableRng};
            use random_helpers::random_station;
            let mut rng = StdRng::seed_from_u64(seed);
            random_station(&mut rng)
        }
        pub(crate) fn dokument_with_seed(seed: u64) -> models::Dokument {
            use rand::rngs::StdRng;
            use rand::{Rng, SeedableRng};
            use random_helpers::random_dokument;
            let mut rng = StdRng::seed_from_u64(seed);
            random_dokument(&mut rng)
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
                schlagworte: Some(vec!["stationär".to_string()]),
                touched_by: None,
                stellungnahmen: Some(vec![models::StationDokumenteInner::Dokument(Box::new(
                    default_stellungnahme(),
                ))]),
                additional_links: Some(vec![
                    "https://example.com/videos/aus/der/hoelle".to_string(),
                ]),
                dokumente: vec![models::StationDokumenteInner::Dokument(Box::new(
                    default_dokument(),
                ))],
                gremium: default_gremium(),
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
                dokumente: Some(vec![models::StationDokumenteInner::Dokument(Box::new(
                    default_dokument(),
                ))]),
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
    }
}
