use md5::{Digest as _, Md5};

use super::*;

use crate::store::Quality;

/// mp3/flac/vorbis точно декодируются; aac/he-aac — нет.
fn supported(codec: &str) -> bool {
    let c = codec.to_ascii_lowercase();
    c.contains("mp3") || c.contains("flac") || c.contains("vorbis") || c.contains("opus")
}

impl Yandex {
    /// Ссылка на get-mp3 по классической схеме download-info + XML + md5-подпись.
    pub async fn stream(&self, track_id: &str, quality: Quality) -> YResult<StreamDto> {
        let result = self
            .get_result(&format!("/tracks/{track_id}/download-info"))
            .await?;
        let mut infos: Vec<YDownloadInfo> =
            serde_json::from_value(result).map_err(|e| ApiError::Msg(e.to_string()))?;
        if infos.is_empty() {
            return Err(ApiError::Msg(
                "Трек недоступен — обычно это значит, что нет подписки Плюс".to_string(),
            ));
        }
        // нереально декодируемые кодеки в конце очереди
        infos.sort_by_key(|i| if supported(&i.codec) { 0 } else { 1 });

        let rank = |i: &YDownloadInfo| -> i64 {
            let br = i.bitrate_in_kbps.unwrap_or(0);
            let bonus = if i.codec.contains("flac") {
                3000
            } else if i.codec.contains("aac") {
                1000
            } else {
                0
            };
            br + bonus
        };

        let best = match quality {
            Quality::Low => infos
                .iter()
                .filter(|i| i.bitrate_in_kbps.unwrap_or(0) <= 128 && !i.codec.contains("flac"))
                .max_by_key(|i| i.bitrate_in_kbps.unwrap_or(0))
                .or_else(|| infos.iter().min_by_key(|i| rank(i))),
            Quality::Normal => infos
                .iter()
                .filter(|i| !i.codec.contains("flac") && i.bitrate_in_kbps.unwrap_or(0) <= 192)
                .max_by_key(|i| i.bitrate_in_kbps.unwrap_or(0))
                .or_else(|| {
                    infos
                        .iter()
                        .filter(|i| !i.codec.contains("flac"))
                        .min_by_key(|i| rank(i))
                }),
            Quality::High => infos
                .iter()
                .filter(|i| !i.codec.contains("flac"))
                .max_by_key(|i| rank(i))
                .or_else(|| infos.iter().max_by_key(|i| rank(i))),
            Quality::Max => infos.iter().max_by_key(|i| rank(i)),
        }
        .ok_or(ApiError::Msg("Нет доступных потоков".to_string()))?;

        build_stream(self, track_id, best).await
    }

    /// Полное тело трека в памяти: 192 kbps mp3 ≈ 5 МБ, это нормально для десктопа.
    pub async fn download(&self, stream: &StreamDto) -> YResult<Arc<Vec<u8>>> {
        let bytes = self.get_bytes(stream.url.clone(), true).await?;
        if bytes.len() < 1024 {
            return Err(ApiError::Msg("Слишком короткий ответ CDN".to_string()));
        }
        Ok(bytes)
    }
}

async fn build_stream(
    api: &Yandex,
    track_id: &str,
    best: &YDownloadInfo,
) -> YResult<StreamDto> {
    let xml = api.get_xml(best.download_info_url.clone()).await?;
    let host = between(&xml, "<host>", "</host>")
        .ok_or_else(|| ApiError::Msg("Некорректный ответ CDN".to_string()))?;
    let path = between(&xml, "<path>", "</path>")
        .ok_or_else(|| ApiError::Msg("Некорректный ответ CDN".to_string()))?;
    let ts = between(&xml, "<ts>", "</ts>")
        .ok_or_else(|| ApiError::Msg("Некорректный ответ CDN".to_string()))?;
    let s = between(&xml, "<s>", "</s>")
        .ok_or_else(|| ApiError::Msg("Некорректный ответ CDN".to_string()))?;

    let sign_raw = format!("{SIGN_SALT}{}{}", &path[1..], s);
    let mut hasher = Md5::new();
    hasher.update(sign_raw.as_bytes());
    let sign = hex::encode(hasher.finalize());
    let url = format!("https://{host}/get-mp3/{sign}/{ts}{path}");

    Ok(StreamDto {
        track_id: track_id.to_string(),
        url,
        codec: best.codec.clone(),
        bitrate: best.bitrate_in_kbps.unwrap_or(0),
    })
}
