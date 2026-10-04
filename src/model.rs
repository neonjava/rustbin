use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Paste {
    pub id: String,
    pub title: Option<String>,
    pub content: String,
    pub language: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing)]
    pub delete_token_hash: String,
    pub views: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePaste {
    pub title: Option<String>,
    pub content: String,
    pub language: String,
    pub expiration: String,
}

pub fn expiration(value: &str, now: DateTime<Utc>) -> Option<Option<DateTime<Utc>>> {
    Some(match value {
        "10m" => Some(now + Duration::minutes(10)),
        "1h" => Some(now + Duration::hours(1)),
        "1d" => Some(now + Duration::days(1)),
        "7d" => Some(now + Duration::days(7)),
        "30d" => Some(now + Duration::days(30)),
        "never" => None,
        _ => return None,
    })
}

pub fn valid_language(s: &str) -> bool {
    matches!(
        s,
        "text"
            | "rust"
            | "java"
            | "c"
            | "cpp"
            | "python"
            | "javascript"
            | "typescript"
            | "html"
            | "css"
            | "json"
            | "yaml"
            | "toml"
            | "bash"
            | "sql"
            | "markdown"
    )
}

pub fn valid_id(s: &str) -> bool {
    (8..=32).contains(&s.len())
        && s.bytes()
            .all(|c| b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789".contains(&c))
}

pub fn expired(p: &Paste) -> bool {
    p.expires_at.is_some_and(|date| date <= Utc::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn input_rules() {
        assert!(valid_language("rust"));
        assert!(!valid_language("<script>"));
        assert!(!valid_id("../../etc"));
        assert!(expiration("1d", Utc::now()).is_some());
        assert!(expiration("bad", Utc::now()).is_none());
    }

    #[test]
    fn expired_pastes_are_hidden() {
        let now = Utc::now();
        let paste = Paste {
            id: "ABCDEFGH".into(),
            title: None,
            content: "test".into(),
            language: "text".into(),
            created_at: now,
            expires_at: Some(now - Duration::seconds(1)),
            delete_token_hash: String::new(),
            views: 0,
        };
        assert!(expired(&paste));
    }
}
