//! Device OAuth Яндекса: тот же поток, что у mashiro (oauth.yandex.ru/device).
//! Публичные константы клиента из GPL-кода mashiro.

use super::*;

pub const OAUTH_CLIENT_ID: &str = "23cabbbdc6cd418abb4b39c32c41195d";
pub const OAUTH_CLIENT_SECRET: &str = "53bc75238f0c4d08a118e51fe9203300";
const DEVICE_CODE_URL: &str = "https://oauth.yandex.ru/device/code";
const TOKEN_URL: &str = "https://oauth.yandex.ru/token";
const DEVICE_ID: &str = "meowsic-desktop-client";
const DEVICE_NAME: &str = "meowsic Desktop";

pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_url: String,
    pub interval: u64,
    pub expires_in: u64,
}

pub enum TokenPoll {
    Pending,
    SlowDown,
    Token(String),
    Failed(String),
}

impl Yandex {
    pub async fn oauth_device_code(&self) -> YResult<DeviceCode> {
        let http = Arc::clone(&self.http);
        let json = spawn_net(async move {
            let resp = http
                .post(DEVICE_CODE_URL)
                .form(&[
                    ("client_id", OAUTH_CLIENT_ID),
                    ("device_id", DEVICE_ID),
                    ("device_name", DEVICE_NAME),
                ])
                .send()
                .await
                .map_err(|e| ApiError::Net(e.to_string()))?;
            resp.json::<Value>().await.map_err(|e| ApiError::Net(e.to_string()))
        })
        .await
        .map_err(ApiError::Net)??;

        if let Some(err) = json.get("error").and_then(|v| v.as_str()) {
            let desc = json
                .get("error_description")
                .and_then(|v| v.as_str())
                .unwrap_or(err);
            return Err(ApiError::Msg(format!("Яндекс отклонил запрос: {desc}")));
        }
        Ok(DeviceCode {
            device_code: json
                .get("device_code")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ApiError::Msg("Нет device_code".to_string()))?
                .to_string(),
            user_code: json
                .get("user_code")
                .and_then(|v| v.as_str())
                .unwrap_or("????")
                .to_string(),
            verification_url: json
                .get("verification_url")
                .and_then(|v| v.as_str())
                .unwrap_or("https://oauth.yandex.ru/device")
                .to_string(),
            interval: json.get("interval").and_then(|v| v.as_u64()).unwrap_or(5).max(1),
            expires_in: json.get("expires_in").and_then(|v| v.as_u64()).unwrap_or(300),
        })
    }

    pub async fn oauth_poll_token(&self, device_code: &str) -> YResult<TokenPoll> {
        let http = Arc::clone(&self.http);
        let device_code = device_code.to_string();
        let json = spawn_net(async move {
            let resp = http
                .post(TOKEN_URL)
                .form(&[
                    ("grant_type", "device_code"),
                    ("code", device_code.as_str()),
                    ("client_id", OAUTH_CLIENT_ID),
                    ("client_secret", OAUTH_CLIENT_SECRET),
                ])
                .send()
                .await
                .map_err(|e| ApiError::Net(e.to_string()))?;
            resp.json::<Value>().await.map_err(|e| ApiError::Net(e.to_string()))
        })
        .await
        .map_err(ApiError::Net)??;

        if let Some(token) = json.get("access_token").and_then(|v| v.as_str()) {
            return Ok(TokenPoll::Token(token.to_string()));
        }
        match json.get("error").and_then(|v| v.as_str()) {
            Some("authorization_pending") => Ok(TokenPoll::Pending),
            Some("slow_down") => Ok(TokenPoll::SlowDown),
            Some(other) => {
                let desc = json
                    .get("error_description")
                    .and_then(|v| v.as_str())
                    .unwrap_or(other)
                    .to_string();
                Ok(TokenPoll::Failed(desc))
            }
            _ => Ok(TokenPoll::Pending),
        }
    }
}
