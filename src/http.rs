//! [`gpui::http_client::HttpClient`] поверх reqwest: нужен, чтобы `img()`
//! подтягивал обложки по URL. Ответы собираются целиком — обложки маленькие.

use std::sync::Arc;

use futures::FutureExt as _;
use gpui_kit::http_client::{AsyncBody, HttpClient, Request, Response, http::HeaderValue};

use crate::net::spawn_net;

pub struct MeowsicHttp {
    inner: reqwest::Client,
    user_agent: HeaderValue,
}

impl MeowsicHttp {
    pub fn new() -> Self {
        let inner = reqwest::Client::builder()
            .user_agent("meowsic/0.1")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client");
        Self {
            inner,
            user_agent: HeaderValue::from_static("meowsic/0.1"),
        }
    }
}

impl HttpClient for MeowsicHttp {
    fn user_agent(&self) -> Option<&HeaderValue> {
        Some(&self.user_agent)
    }

    fn proxy(&self) -> Option<&gpui_kit::http_client::Url> {
        None
    }

    fn send(
        &self,
        req: Request<AsyncBody>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Response<AsyncBody>>> + Send + 'static>>
    {
        let client = self.inner.clone();
        async move {
            let (head, _body) = req.into_parts();
            let url = head.uri.to_string();
            let method = head.method.as_str().to_string();
            let mut headers = reqwest::header::HeaderMap::new();
            for (name, value) in head.headers.iter() {
                if let (Ok(name), Ok(value)) = (
                    reqwest::header::HeaderName::from_bytes(name.as_ref()),
                    reqwest::header::HeaderValue::from_bytes(value.as_bytes()),
                ) {
                    headers.insert(name, value);
                }
            }
            let inner: anyhow::Result<Vec<u8>> = spawn_net(async move {
                let method = reqwest::Method::from_bytes(method.as_bytes())?;
                let resp = client
                    .request(method, &url)
                    .headers(headers)
                    .send()
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;
                let status = resp.status();
                if !status.is_success() {
                    return Err(anyhow::anyhow!("http {status}"));
                }
                Ok(resp.bytes().await?.to_vec())
            })
            .await
            .map_err(anyhow::Error::msg)?;
            let bytes = inner?;
            Ok(Response::builder()
                .status(200)
                .header("content-type", "application/octet-stream")
                .body(AsyncBody::from(bytes))?)
        }
        .boxed()
    }
}

pub fn http_client() -> Arc<MeowsicHttp> {
    Arc::new(MeowsicHttp::new())
}
