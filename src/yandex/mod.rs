mod account;
mod catalog;
mod dto;
mod library;
mod lyrics;
mod stream;
mod wave;

pub use dto::*;

/// Схема сырых ответов Яндекса — размытая, всё необязательное.
mod raw {
    use serde::Deserialize;

    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct YArtist {
        pub id: serde_json::Value,
        pub name: Option<String>,
        pub cover: Option<YCover>,
        pub og_image: Option<String>,
    }

    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct YAlbum {
        pub id: serde_json::Value,
        pub title: Option<String>,
        pub cover_uri: Option<String>,
        pub year: Option<i64>,
    }

    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct YTrack {
        pub id: serde_json::Value,
        pub title: Option<String>,
        pub artists: Vec<YArtist>,
        pub albums: Vec<YAlbum>,
        pub cover_uri: Option<String>,
        pub duration_ms: Option<i64>,
        pub available: Option<bool>,
        pub content_warning: Option<String>,
    }

    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct YTrackWrap {
        pub track: Option<YTrack>,
    }

    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct YCover {
        pub uri: Option<String>,
    }

    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct YOwner {
        pub uid: Option<i64>,
    }

    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct YPlaylist {
        pub kind: i64,
        pub title: Option<String>,
        pub track_count: Option<i64>,
        pub cover: Option<YCover>,
        pub og_image: Option<String>,
        pub owner: Option<YOwner>,
    }

    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct YAccount {
        pub uid: Option<i64>,
        pub login: Option<String>,
        pub display_name: Option<String>,
        pub full_name: Option<String>,
        pub second_name: Option<String>,
        pub email: Option<String>,
        pub avatar_url: Option<String>,
        pub avatar_id: Option<String>,
    }

    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct YPlus {
        pub has_plus: Option<bool>,
    }

    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct YAccountStatus {
        pub account: YAccount,
        pub plus: Option<YPlus>,
    }

    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct YDownloadInfo {
        pub codec: String,
        pub bitrate_in_kbps: Option<i64>,
        pub download_info_url: String,
    }

    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct YRotorSession {
        pub radio_session_id: Option<String>,
        pub batch_id: Option<String>,
        pub sequence: Vec<YTrackWrap>,
    }

    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct YPlaylistFull {
        pub tracks: Vec<YTrackWrap>,
    }
}

pub(crate) use raw::*;

use std::sync::Arc;

use serde_json::Value;

use crate::net::spawn_net;

pub(crate) const BASE: &str = "https://api.music.yandex.net";
pub(crate) const SIGN_SALT: &str = "XGRlBW9FXlekgbPrRHuSiA";
pub(crate) const MUSIC_CLIENT: &str = "YandexMusicAndroid/24023621";

#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    Http(u16),
    NoPlus,
    Empty,
    Msg(String),
    Net(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Unauthorized => f.write_str("Невалидный токен Яндекс Музыки"),
            ApiError::Http(code) => write!(f, "Ошибка API Яндекса: {code}"),
            ApiError::NoPlus => f.write_str("Нужна подписка Плюс для полного воспроизведения"),
            ApiError::Empty => f.write_str("Ничего не найдено"),
            ApiError::Msg(m) => f.write_str(m),
            ApiError::Net(m) => write!(f, "Сеть недоступна: {m}"),
        }
    }
}

impl std::error::Error for ApiError {}

pub type YResult<T> = Result<T, ApiError>;

pub(crate) fn err<T>(e: reqwest::Error) -> YResult<T> {
    Err(ApiError::Net(e.to_string()))
}

/// Неофициальный клиент Яндекс Музыки. Дёшево клонируется: токен и reqwest
/// живут в общих ручках, смена токена видна всем держателям.
#[derive(Clone)]
pub struct Yandex {
    http: Arc<reqwest::Client>,
    token: Arc<std::sync::Mutex<String>>,
}

impl Yandex {
    pub fn new(token: &str) -> Self {
        let http = reqwest::Client::builder()
            .user_agent("Yandex-Music-API")
            .build()
            .expect("reqwest client");
        Self {
            http: Arc::new(http),
            token: Arc::new(std::sync::Mutex::new(clean_token(token))),
        }
    }

    pub fn set_token(&self, token: &str) {
        *self.token.lock().unwrap() = clean_token(token);
    }

    pub fn token(&self) -> String {
        self.token.lock().unwrap().clone()
    }

    pub fn has_token(&self) -> bool {
        !self.token().is_empty()
    }

    fn auth(&self) -> String {
        format!("OAuth {}", self.token())
    }

    /// GET {BASE}{path} → result. Всё крутится на tokio-рантайме.
    pub(crate) async fn get_result(&self, path: &str) -> YResult<Value> {
        self.request("GET", path, None, Vec::new()).await
    }

    pub(crate) async fn post_value(&self, path: &str, body: Value) -> YResult<Value> {
        self.request("POST", path, Some(body), Vec::new()).await
    }

    pub(crate) async fn post_json(&self, path: &str) -> YResult<Value> {
        self.request("POST", path, Some(Value::Object(Default::default())), Vec::new())
            .await
    }

    pub(crate) async fn post_form(
        &self,
        path: &str,
        params: &[(&'static str, String)],
    ) -> YResult<Value> {
        let (http, auth, url) = self.parts(&format!("{BASE}{path}"));
        let params: Vec<(String, String)> =
            params.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
        spawn_net(async move {
            let resp = http
                .post(&url)
                .header("Authorization", auth)
                .header("X-Yandex-Music-Client", MUSIC_CLIENT)
                .form(&params)
                .send()
                .await
                .map_err(|e| ApiError::Net(e.to_string()))?;
            parse_json_result(resp).await
        })
        .await
        .map_err(ApiError::Net)?
    }

    /// POST сырых form-параметров, ответ не обязательен JSON — статус.
    pub(crate) async fn post_status(
        &self,
        path: &str,
        content_type: &str,
        body: String,
    ) -> YResult<u16> {
        let (http, auth, url) = self.parts(&format!("{BASE}{path}"));
        let ct = content_type.to_string();
        spawn_net(async move {
            let resp = http
                .post(&url)
                .header("Authorization", auth)
                .header("X-Yandex-Music-Client", MUSIC_CLIENT)
                .header("Content-Type", ct)
                .body(body)
                .send()
                .await
                .map_err(|e| ApiError::Net(e.to_string()))?;
            Ok(resp.status().as_u16())
        })
        .await
        .map_err(ApiError::Net)?
    }

    async fn request(&self, verb: &str, path: &str, body: Option<Value>, extra: Vec<(String, String)>) -> YResult<Value> {
        let (http, auth, url) = self.parts(&format!("{BASE}{path}"));
        let is_post = verb == "POST";
        let headers: Vec<(String, String)> = extra;
        spawn_net(async move {
            let mut req = match (is_post, body) {
                (true, Some(v)) => http
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .header("X-Yandex-Music-Client", MUSIC_CLIENT)
                    .body(v.to_string()),
                (true, None) => http
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .body("{}"),
                (false, _) => http.get(&url),
            };
            for (k, v) in headers {
                req = req.header(k, v);
            }
            let resp = req
                .header("Authorization", auth)
                .send()
                .await
                .map_err(|e| ApiError::Net(e.to_string()))?;
            parse_json_result(resp).await
        })
        .await
        .map_err(ApiError::Net)?
    }

    /// XML-ответ download-info без авторизации.
    pub(crate) async fn get_xml(&self, url: String) -> YResult<String> {
        let http = Arc::clone(&self.http);
        let auth = self.auth();
        spawn_net(async move {
            let resp = http
                .get(&url)
                .header("Authorization", auth)
                .send()
                .await
                .map_err(|e| ApiError::Net(e.to_string()))?;
            if !resp.status().is_success() {
                return Err(ApiError::Http(resp.status().as_u16()));
            }
            Ok(resp.text().await.map_err(|e| ApiError::Net(e.to_string()))?)
        })
        .await
        .map_err(ApiError::Net)?
    }

    /// Скачивание байтов потока/картинки; возвращаемые данные уже целиком в памяти.
    pub(crate) async fn get_bytes(&self, url: String, client_header: bool) -> YResult<Arc<Vec<u8>>> {
        let http = Arc::clone(&self.http);
        let auth = self.auth();
        spawn_net(async move {
            let mut req = http.get(&url);
            if url.contains("api.music.yandex") {
                req = req.header("Authorization", auth);
            }
            if client_header {
                req = req.header("X-Yandex-Music-Client", MUSIC_CLIENT);
            }
            let resp = req.send().await.map_err(|e| ApiError::Net(e.to_string()))?;
            if !resp.status().is_success() {
                return Err(ApiError::Http(resp.status().as_u16()));
            }
            Ok(Arc::new(resp.bytes().await.map_err(|e| ApiError::Net(e.to_string()))?.to_vec()))
        })
        .await
        .map_err(ApiError::Net)?
    }

    /// Сторонний JSON (lrclib и т.п.) без яндекс-заголовков.
    pub(crate) async fn get_external_json(&self, url: String) -> YResult<Value> {
        let http = Arc::clone(&self.http);
        spawn_net(async move {
            let resp = http
                .get(&url)
                .header("Accept", "application/json")
                .header("User-Agent", "meowsic/0.1 (https://github.com/meowsic)")
                .send()
                .await
                .map_err(|e| ApiError::Net(e.to_string()))?;
            if !resp.status().is_success() {
                return Err(ApiError::Http(resp.status().as_u16()));
            }
            Ok(resp.json::<Value>().await.map_err(|e| ApiError::Net(e.to_string()))?)
        })
        .await
        .map_err(ApiError::Net)?
    }

    fn parts(&self, url: &str) -> (reqwest::Client, String, String) {
        ((&*self.http).clone(), self.auth(), url.to_string())
    }
}

async fn parse_json_result(resp: reqwest::Response) -> YResult<Value> {
    let status = resp.status();
    if status.as_u16() == 401 {
        return Err(ApiError::Unauthorized);
    }
    let body: Value = resp.json().await.map_err(|e| ApiError::Net(e.to_string()))?;
    if !status.is_success() {
        return Err(ApiError::Http(status.as_u16()));
    }
    Ok(body.get("result").cloned().unwrap_or(Value::Null))
}

fn clean_token(raw: &str) -> String {
    let t = raw.trim();
    t.strip_prefix("Bearer ")
        .or_else(|| t.strip_prefix("bearer "))
        .or_else(|| t.strip_prefix("OAuth "))
        .unwrap_or(t)
        .to_string()
}

// --- мелкие хелперы ---

pub(crate) fn val_to_id(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub(crate) fn img(uri: Option<&str>, size: &str) -> Option<String> {
    uri.filter(|u| !u.is_empty()).map(|u| {
        let replaced = u.replace("%%", size);
        if replaced.starts_with("http") {
            replaced
        } else {
            format!("https://{replaced}")
        }
    })
}

pub(crate) fn cover_url(uri: &Option<String>) -> Option<String> {
    img(uri.as_deref(), "200x200")
}

pub(crate) fn between<'a>(s: &'a str, a: &str, b: &str) -> Option<&'a str> {
    let start = s.find(a)? + a.len();
    let end = s[start..].find(b)? + start;
    Some(&s[start..end])
}

pub(crate) fn str_at(v: &Value, path: &[&str]) -> Option<String> {
    let mut cur = v;
    for key in path {
        cur = cur.get(*key)?;
    }
    cur.as_str().map(|s| s.to_string())
}

#[cfg(test)]
mod net_tests {
    use super::*;

    /// Сеть до API Яндекса работает и разбор ответа живой, даже без токена:
    /// ждём либо валидный result, либо аккуратную 401-ошибку — не панику.
    /// Публичный.lrclib как канарейка сетевого пути: TLS, парсинг, статусы.
    #[test]
    fn lrclib_reachable() {
        let api = Yandex::new("");
        let res = crate::net::RUNTIME.block_on(api.get_external_json(
            "https://lrclib.net/api/search?artist_name=Molchatova&track_name=Ya+tvoyu".to_string(),
        ));
        match res {
            Ok(v) => assert!(v.is_array(), "lrclib вернул не массив: {v}"),
            Err(e) => panic!("сеть/парсинг сломались: {e}"),
        }
    }

    /// Статус без токена обязан возвращаться как Unauthorized.
    #[test]
    fn bad_token_maps_to_unauthorized() {
        let api = Yandex::new("invalid-token-for-test");
        let res = crate::net::RUNTIME.block_on(api.account_status());
        assert!(matches!(res, Err(ApiError::Unauthorized)));
    }
}
