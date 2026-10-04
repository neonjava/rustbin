use super::PasteStore;
use crate::model::Paste;
use anyhow::{Context, anyhow};
use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use std::{
    env,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Firestore {
    client: Client,
    base: String,
    emulator: bool,
    token_cache: Arc<Mutex<Option<(Instant, String)>>>,
}

impl Firestore {
    pub fn new(project: &str, emulator: Option<&str>) -> Self {
        let (base, emulated) = match emulator {
            Some(host) => (format!("http://{host}"), true),
            None => ("https://firestore.googleapis.com".to_owned(), false),
        };
        Self {
            client: Client::new(),
            base: format!("{base}/v1/projects/{project}/databases/(default)/documents/pastes"),
            emulator: emulated,
            token_cache: Arc::new(Mutex::new(None)),
        }
    }

    async fn access_token(&self) -> anyhow::Result<Option<String>> {
        if self.emulator {
            return Ok(None);
        }
        let mut cache = self.token_cache.lock().await;
        if let Some((until, token)) = cache.as_ref()
            && Instant::now() < *until
        {
            return Ok(Some(token.clone()));
        }
        let configured = env::var_os("GOOGLE_APPLICATION_CREDENTIALS").map(PathBuf::from);
        let (token, seconds) = if let Some(path) = configured {
            self.file_access_token(path).await?
        } else {
            let metadata = self.client.get("http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token")
                .header("Metadata-Flavor", "Google").timeout(Duration::from_secs(2)).send().await;
            if let Ok(response) = metadata
                && response.status().is_success()
            {
                token_fields(response.json().await?)?
            } else {
                let path = env::var_os("HOME")
                    .map(|home| {
                        PathBuf::from(home)
                            .join(".config/gcloud/application_default_credentials.json")
                    })
                    .context("Application Default Credentials not found")?;
                self.file_access_token(path).await?
            }
        };
        *cache = Some((
            Instant::now() + Duration::from_secs(seconds.saturating_sub(60)),
            token.clone(),
        ));
        Ok(Some(token))
    }

    async fn file_access_token(&self, path: PathBuf) -> anyhow::Result<(String, u64)> {
        let credentials: Value = serde_json::from_slice(&tokio::fs::read(path).await?)?;
        let response = match credentials["type"].as_str() {
            Some("authorized_user") => {
                let form = [
                    ("grant_type", "refresh_token"),
                    (
                        "client_id",
                        credentials["client_id"]
                            .as_str()
                            .context("client_id missing")?,
                    ),
                    (
                        "client_secret",
                        credentials["client_secret"]
                            .as_str()
                            .context("client_secret missing")?,
                    ),
                    (
                        "refresh_token",
                        credentials["refresh_token"]
                            .as_str()
                            .context("refresh_token missing")?,
                    ),
                ];
                self.client
                    .post("https://oauth2.googleapis.com/token")
                    .form(&form)
                    .send()
                    .await?
            }
            Some("service_account") => {
                let email = credentials["client_email"]
                    .as_str()
                    .context("client_email missing")?;
                let pem = credentials["private_key"]
                    .as_str()
                    .context("private_key missing")?;
                let now = Utc::now().timestamp();
                let mut header = Header::new(Algorithm::RS256);
                header.kid = credentials["private_key_id"].as_str().map(str::to_owned);
                let claims = json!({
                    "iss": email,
                    "scope": "https://www.googleapis.com/auth/datastore",
                    "aud": "https://oauth2.googleapis.com/token",
                    "iat": now,
                    "exp": now + 3600
                });
                let assertion = encode(
                    &header,
                    &claims,
                    &EncodingKey::from_rsa_pem(pem.as_bytes())?,
                )?;
                let form = [
                    ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                    ("assertion", assertion.as_str()),
                ];
                self.client
                    .post("https://oauth2.googleapis.com/token")
                    .form(&form)
                    .send()
                    .await?
            }
            _ => anyhow::bail!("unsupported Application Default Credentials type"),
        };
        token_fields(response.error_for_status()?.json().await?)
    }

    fn request(
        &self,
        method: reqwest::Method,
        url: &str,
        token: Option<String>,
    ) -> reqwest::RequestBuilder {
        let builder = self.client.request(method, url);
        if let Some(token) = token {
            builder.bearer_auth(token)
        } else {
            builder
        }
    }
}

fn token_fields(value: Value) -> anyhow::Result<(String, u64)> {
    Ok((
        value["access_token"]
            .as_str()
            .context("access token missing")?
            .to_owned(),
        value["expires_in"]
            .as_u64()
            .context("token expiry missing")?,
    ))
}

fn text(v: &Value, name: &str) -> anyhow::Result<String> {
    Ok(v["fields"][name]["stringValue"]
        .as_str()
        .with_context(|| format!("missing {name}"))?
        .to_owned())
}
fn timestamp(v: &Value, name: &str) -> anyhow::Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(
        v["fields"][name]["timestampValue"]
            .as_str()
            .context("timestamp missing")?,
    )?
    .with_timezone(&Utc))
}
fn decode(v: Value) -> anyhow::Result<Paste> {
    let expires_at = v["fields"]["expires_at"]["timestampValue"]
        .as_str()
        .map(|s| DateTime::parse_from_rfc3339(s).map(|t| t.with_timezone(&Utc)))
        .transpose()?;
    Ok(Paste {
        id: v["name"]
            .as_str()
            .context("document name missing")?
            .rsplit('/')
            .next()
            .context("id missing")?
            .to_owned(),
        title: v["fields"]["title"]["stringValue"]
            .as_str()
            .map(str::to_owned),
        content: text(&v, "content")?,
        language: text(&v, "language")?,
        created_at: timestamp(&v, "created_at")?,
        expires_at,
        delete_token_hash: text(&v, "delete_token_hash")?,
        views: v["fields"]["views"]["integerValue"]
            .as_str()
            .context("views missing")?
            .parse()?,
    })
}

impl PasteStore for Firestore {
    async fn create(&self, paste: Paste) -> anyhow::Result<bool> {
        let url = format!("{}?documentId={}", self.base, paste.id);
        let mut fields = json!({
            "content":{"stringValue":paste.content}, "language":{"stringValue":paste.language},
            "created_at":{"timestampValue":paste.created_at.to_rfc3339()},
            "delete_token_hash":{"stringValue":paste.delete_token_hash}, "views":{"integerValue":"0"}
        });
        if let Some(title) = paste.title {
            fields["title"] = json!({"stringValue":title});
        }
        if let Some(date) = paste.expires_at {
            fields["expires_at"] = json!({"timestampValue":date.to_rfc3339()});
        }
        let response = self
            .request(reqwest::Method::POST, &url, self.access_token().await?)
            .json(&json!({"fields":fields}))
            .send()
            .await?;
        match response.status() {
            StatusCode::OK => Ok(true),
            StatusCode::CONFLICT => Ok(false),
            status => Err(anyhow!("Firestore create status {status}")),
        }
    }

    async fn get(&self, id: &str) -> anyhow::Result<Option<Paste>> {
        let url = format!("{}/{id}", self.base);
        let response = self
            .request(reqwest::Method::GET, &url, self.access_token().await?)
            .send()
            .await?;
        match response.status() {
            StatusCode::OK => Ok(Some(decode(response.json().await?)?)),
            StatusCode::NOT_FOUND => Ok(None),
            status => Err(anyhow!("Firestore get status {status}")),
        }
    }

    async fn delete(&self, id: &str, token_hash: &str) -> anyhow::Result<bool> {
        // Verify against the stored hash; the random token itself never enters Firestore.
        let Some(paste) = self.get(id).await? else {
            return Ok(false);
        };
        if paste.delete_token_hash != token_hash {
            return Ok(false);
        }
        let url = format!("{}/{id}", self.base);
        let response = self
            .request(reqwest::Method::DELETE, &url, self.access_token().await?)
            .send()
            .await?;
        anyhow::ensure!(
            response.status().is_success(),
            "Firestore delete status {}",
            response.status()
        );
        Ok(true)
    }
}
