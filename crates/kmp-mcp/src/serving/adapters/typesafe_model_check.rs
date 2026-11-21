//! `doctor`'s check that a store's pinned TypeSafe model is still served.
//! It sends the key and nothing else: no state, no question, no store text.

use std::path::Path;
use std::time::Duration;

use reqwest::header::AUTHORIZATION;
use serde_json::Value;

use super::typesafe_api_key::TypeSafeApiKey;
use super::typesafe_judgement::read_config;
use crate::serving::environment::TYPESAFE_API_KEY_ENV;
use crate::serving::model_check::ModelCheck;

/// How long the check waits, at most: a slow service is "not checked".
const TIMEOUT: Duration = Duration::from_secs(3);
/// The most of the list that is read.
const LIST_BYTES: usize = 64 * 1024;

/// The check for the store at `data_dir`: `None` when it has no
/// `typesafe.json` or the file does not validate (the store config section
/// already says why).
pub(crate) fn pinned_model_check(data_dir: &Path) -> Option<ModelCheck> {
    let config = read_config(data_dir).ok()??;
    let (endpoint, _) = config.validate().ok()?;
    let model = config.model;
    let key = match TypeSafeApiKey::load(std::env::var(TYPESAFE_API_KEY_ENV).ok()) {
        Ok(key) => key,
        Err(reason) => return Some(ModelCheck::NotChecked { model, reason }),
    };
    let mut models = endpoint;
    models.set_path("/v1/models");
    Some(check_models(models, key, model))
}

/// Asks `url` for the served models with `key` and reads whether `model` is
/// among them. Runs on its own thread, so it can be called from inside an
/// async runtime.
pub(super) fn check_models(url: reqwest::Url, key: TypeSafeApiKey, model: String) -> ModelCheck {
    let asked = model.clone();
    let listed = std::thread::spawn(move || list_models(url, &key))
        .join()
        .unwrap_or_else(|_| Err("the check failed".to_string()));
    match listed {
        Ok(served) if served.contains(&asked) => ModelCheck::Served { model },
        Ok(served) => ModelCheck::NotServed { model, served },
        Err(reason) => ModelCheck::NotChecked { model, reason },
    }
}

fn list_models(url: reqwest::Url, key: &TypeSafeApiKey) -> Result<Vec<String>, String> {
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .build()
        .map_err(|_| "cannot create the client".to_string())?;
    let response = client
        .get(url)
        .header(AUTHORIZATION, key.header())
        .send()
        .map_err(|error| {
            if error.is_timeout() {
                "TypeSafe did not answer within 3 s".to_string()
            } else {
                "TypeSafe unreachable".to_string()
            }
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("TypeSafe answered HTTP {}", status.as_u16()));
    }
    let bytes = response
        .bytes()
        .map_err(|_| "TypeSafe answer interrupted".to_string())?;
    if bytes.len() > LIST_BYTES {
        return Err("TypeSafe model list exceeds 64 KiB".into());
    }
    let list: Value =
        serde_json::from_slice(&bytes).map_err(|_| "invalid TypeSafe model list".to_string())?;
    let models = list["models"]
        .as_array()
        .ok_or("invalid TypeSafe model list")?
        .iter()
        .filter_map(|entry| entry["name"].as_str().map(str::to_string))
        .collect::<Vec<_>>();
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::super::typesafe_fixture::{Reply, serve};
    use super::*;

    const KEY: &str = "apikey_fixture_secret";

    fn reply(status: u16, body: &str) -> Reply {
        Reply {
            status,
            headers: Vec::new(),
            body: body.into(),
            delay: Duration::ZERO,
        }
    }

    fn check(replies: Vec<Reply>) -> (ModelCheck, Vec<(String, Value)>) {
        let (url, fixture) = serve(replies);
        let mut models = url;
        models.set_path("/v1/models");
        let key = TypeSafeApiKey::from_env(Some(KEY.into())).expect("key");
        let checked = check_models(models, key, "jev-1.14.0".into());
        (checked, fixture.join().expect("fixture"))
    }

    #[test]
    fn a_served_pin_reads_served_and_only_the_key_is_sent() {
        let (checked, received) = check(vec![reply(
            200,
            r#"{"models":[{"name":"jev-1.14.0","description":"d","release_date":"2026-10-14"},{"name":"jev-latest","description":"d","release_date":"2026-10-14"}]}"#,
        )]);
        assert_eq!(
            checked,
            ModelCheck::Served {
                model: "jev-1.14.0".into()
            }
        );
        let (headers, body) = &received[0];
        assert!(headers.starts_with("GET /v1/models"), "{headers}");
        assert!(
            headers
                .to_ascii_lowercase()
                .contains(&format!("bearer {KEY}"))
        );
        assert_eq!(*body, Value::Null, "no body: no store text");
    }

    #[test]
    fn a_pin_missing_from_the_list_reads_not_served() {
        let (checked, _) = check(vec![reply(200, r#"{"models":[{"name":"jev-1.15.0"}]}"#)]);
        assert_eq!(
            checked,
            ModelCheck::NotServed {
                model: "jev-1.14.0".into(),
                served: vec!["jev-1.15.0".into()]
            }
        );
    }

    #[test]
    fn a_refusal_or_no_service_is_not_checked_never_a_verdict() {
        let (checked, _) = check(vec![reply(529, "{}")]);
        assert!(matches!(checked, ModelCheck::NotChecked { reason, .. } if reason.contains("529")));
        let (checked, _) = check(Vec::new());
        assert!(matches!(checked, ModelCheck::NotChecked { .. }));
    }

    #[test]
    fn a_store_without_typesafe_json_is_not_asked() {
        let directory = tempfile::tempdir().expect("store");
        assert_eq!(pinned_model_check(directory.path()), None);
    }
}
