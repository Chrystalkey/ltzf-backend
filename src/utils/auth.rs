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
