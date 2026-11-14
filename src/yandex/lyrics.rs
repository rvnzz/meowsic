use super::*;

/// Разбор LRC: строки `[mm:ss.xx]текст`, без таймстемпов — обычное время 0.
pub fn parse_lrc(raw: &str) -> Vec<LyricsLineDto> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('[') {
            if let Some((stamp, text)) = rest.split_once(']') {
                if let Some(ms) = parse_stamp(stamp) {
                    out.push(LyricsLineDto {
                        time_ms: ms,
                        text: text.trim().to_string(),
                    });
                    continue;
                }
                if matches!(
                    stamp.split(':').next().unwrap_or("").to_ascii_lowercase().as_str(),
                    "ar" | "al" | "ti" | "au" | "by" | "re" | "ve" | "length" | "offset"
                ) {
                    continue;
                }
                out.push(LyricsLineDto {
                    time_ms: 0,
                    text: trimmed.to_string(),
                });
                continue;
            }
        }
        out.push(LyricsLineDto {
            time_ms: 0,
            text: trimmed.to_string(),
        });
    }
    if out.iter().any(|line| line.time_ms > 0) {
        out.sort_by_key(|line| line.time_ms);
    }
    out
}

pub fn parse_stamp(stamp: &str) -> Option<i64> {
    let (mm, rest) = stamp.split_once(':')?;
    let minutes: i64 = mm.trim().parse().ok()?;
    let seconds: f64 = rest.replace(',', ".").trim().parse().ok()?;
    Some(minutes * 60_000 + (seconds * 1000.0).round() as i64)
}

impl Yandex {
    /// Тексты из lrclib: сначала точный матчинг, потом поиск с выбором
    /// ближайшего по длительности, синхронный в приоритете.
    pub async fn lyrics_lrclib(&self, track: &TrackDto) -> YResult<LyricsDto> {
        let artist = track
            .artists
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_default();
        let album = track.album_title.clone().unwrap_or_default();
        let duration = (track.duration_ms.unwrap_or(0) / 1000).max(0);
        if artist.is_empty() || track.title.is_empty() {
            return Err(ApiError::Empty);
        }

        let pick = |body: &serde_json::Value| -> Option<(String, bool)> {
            let synced = body
                .get("syncedLyrics")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            match synced {
                Some(s) => Some((s, true)),
                None => body
                    .get("plainLyrics")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| (s.to_string(), false)),
            }
        };

        let exact = format!(
            "https://lrclib.net/api/get?artist_name={}&track_name={}&album_name={}&duration={duration}",
            urlencoding::encode(&artist),
            urlencoding::encode(&track.title),
            urlencoding::encode(&album),
        );
        if let Ok(body) = self.get_external_json(exact).await {
            if let Some((text, _)) = pick(&body) {
                return Ok(build_lyrics(&track.id, &text));
            }
        }

        let search = format!(
            "https://lrclib.net/api/search?artist_name={}&track_name={}",
            urlencoding::encode(&artist),
            urlencoding::encode(&track.title),
        );
        let body = self.get_external_json(search).await?;
        let items = body
            .as_array()
            .cloned()
            .ok_or(ApiError::Msg("LRCLIB не нашёл текст".to_string()))?;

        let mut best: Option<(i64, bool, String)> = None;
        for item in &items {
            let Some((text, synced)) = pick(item) else { continue };
            let item_duration = item
                .get("duration")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
                .round() as i64;
            let delta = (item_duration - duration).abs();
            let better = match &best {
                None => true,
                Some((bd, bs, _)) => (synced && !*bs) || (synced == *bs && delta < *bd),
            };
            if better {
                best = Some((delta, synced, text));
            }
        }

        match best {
            Some((_, _, text)) => Ok(build_lyrics(&track.id, &text)),
            None => Err(ApiError::Msg("LRCLIB не нашёл текст".to_string())),
        }
    }
}

fn build_lyrics(track_id: &str, text: &str) -> LyricsDto {
    let lines = parse_lrc(text);
    let synced = lines.iter().any(|l| l.time_ms > 0);
    LyricsDto {
        track_id: track_id.to_string(),
        synced,
        source: "lrclib".to_string(),
        lines,
    }
}
