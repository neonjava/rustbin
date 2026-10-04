use std::{collections::HashMap, env, net::IpAddr};

#[derive(Clone)]
pub struct Config {
    pub host: IpAddr,
    pub port: u16,
    pub project_id: String,
    pub max_paste_size: usize,
    pub id_length: usize,
    pub rate_limit: usize,
    pub firestore_emulator: Option<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let local: HashMap<String, String> = std::fs::read_to_string(".env")
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                let (key, value) = line.split_once('=')?;
                let key = key.trim();
                (!key.is_empty() && !key.starts_with('#'))
                    .then(|| (key.to_owned(), value.trim().to_owned()))
            })
            .collect();
        let get = |key: &str| env::var(key).ok().or_else(|| local.get(key).cloned());
        let number = |key: &str, default: usize| -> anyhow::Result<usize> {
            Ok(get(key)
                .map(|value| value.parse())
                .transpose()?
                .unwrap_or(default))
        };
        let project_id = get("FIREBASE_PROJECT_ID")
            .or_else(|| get("GOOGLE_CLOUD_PROJECT"))
            .ok_or_else(|| anyhow::anyhow!("FIREBASE_PROJECT_ID is required"))?;
        let id_length = number("RUSTBIN_ID_LENGTH", 8)?;
        anyhow::ensure!(
            (8..=32).contains(&id_length),
            "RUSTBIN_ID_LENGTH must be 8..=32"
        );
        let max_paste_size = number("RUSTBIN_MAX_PASTE_SIZE", 524_288)?;
        anyhow::ensure!(
            max_paste_size > 0 && max_paste_size <= 900_000,
            "invalid max paste size"
        );
        Ok(Self {
            host: get("RUSTBIN_HOST")
                .unwrap_or_else(|| "0.0.0.0".into())
                .parse()?,
            port: get("PORT").unwrap_or_else(|| "8080".into()).parse()?,
            project_id,
            max_paste_size,
            id_length,
            rate_limit: number("RUSTBIN_RATE_LIMIT", 30)?.max(1),
            firestore_emulator: get("FIRESTORE_EMULATOR_HOST"),
        })
    }
}
