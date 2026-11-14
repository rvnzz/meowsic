use super::*;

impl Yandex {
    pub async fn search(&self, text: &str) -> YResult<SearchDto> {
        let encoded = urlencoding::encode(text);
        let result = self
            .get_result(&format!("/search?text={encoded}&type=all&page=0&nocorrect=false"))
            .await?;

        let take = |key: &str, limit: usize| -> Vec<serde_json::Value> {
            result
                .get(key)
                .and_then(|t| t.get("results"))
                .and_then(|r| r.as_array())
                .map(|arr| arr.iter().take(limit).cloned().collect())
                .unwrap_or_default()
        };

        Ok(SearchDto {
            text: text.to_string(),
            tracks: map_tracks(&take("tracks", 60)),
            artists: take("artists", 12)
                .iter()
                .filter_map(|raw| serde_json::from_value::<YArtist>(raw.clone()).ok())
                .map(|a| map_artist(&a))
                .collect(),
            albums: take("albums", 12)
                .iter()
                .filter_map(|raw| serde_json::from_value::<YAlbum>(raw.clone()).ok())
                .map(|a| map_album_brief(&a))
                .collect(),
        })
    }

    pub async fn search_suggest(&self, part: &str) -> YResult<Vec<String>> {
        let part = part.trim();
        if part.is_empty() {
            return Ok(Vec::new());
        }
        let result = self
            .get_result(&format!("/search/suggest?part={}", urlencoding::encode(part)))
            .await?;
        let mut out = Vec::new();
        if let Some(best) = result
            .get("best")
            .and_then(|b| b.get("text"))
            .and_then(|v| v.as_str())
        {
            out.push(best.to_string());
        }
        if let Some(arr) = result.get("suggestions").and_then(|v| v.as_array()) {
            for item in arr {
                let text = match item {
                    serde_json::Value::String(v) => Some(v.clone()),
                    other => other.get("text").and_then(|v| v.as_str()).map(str::to_string),
                };
                if let Some(text) = text {
                    out.push(text);
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        out.retain(|item| !item.trim().is_empty() && seen.insert(item.to_lowercase()));
        out.truncate(10);
        Ok(out)
    }

    pub async fn artist(&self, artist_id: &str) -> YResult<ArtistPageDto> {
        let result = self
            .get_result(&format!("/artists/{artist_id}/brief-info"))
            .await?;
        let artist = result.get("artist").cloned().unwrap_or_default();
        let name = artist
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Артист")
            .to_string();
        let cover = str_at(&artist, &["cover", "uri"]);
        let description = str_at(&artist, &["description", "text"]);
        let listeners = result
            .get("stats")
            .and_then(|s| s.get("lastMonthListeners"))
            .and_then(|v| v.as_i64());
        let genres = artist
            .get("genres")
            .and_then(|g| g.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let tracks = result
            .get("popularTracks")
            .and_then(|t| t.as_array())
            .map(|arr| arr.iter().take(30).cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let mut albums = Vec::new();
        for key in ["albums", "alsoAlbums"] {
            if let Some(arr) = result.get(key).and_then(|a| a.as_array()) {
                for raw in arr {
                    if let Ok(a) = serde_json::from_value::<YAlbum>(raw.clone()) {
                        albums.push(map_album_brief(&a));
                    }
                }
            }
        }
        Ok(ArtistPageDto {
            id: artist_id.to_string(),
            name,
            cover_url: img(cover.as_deref(), "400x400"),
            description,
            listeners,
            genres,
            tracks: map_tracks(&tracks),
            albums,
        })
    }

    pub async fn artist_tracks(&self, artist_id: &str) -> YResult<Vec<TrackDto>> {
        let mut tracks = Vec::new();
        for page in 0..5 {
            let result = self
                .get_result(&format!(
                    "/artists/{artist_id}/tracks?page={page}&page-size=100"
                ))
                .await?;
            let items = result
                .get("tracks")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let count = items.len();
            tracks.extend(map_tracks(&items));
            if count < 100 {
                break;
            }
        }
        Ok(tracks)
    }

    pub async fn album(&self, album_id: &str) -> YResult<AlbumPageDto> {
        let result = self
            .get_result(&format!("/albums/{album_id}/with-tracks"))
            .await?;
        let title = result
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Альбом")
            .to_string();
        let cover = str_at(&result, &["cover", "uri"])
            .or_else(|| result.get("coverUri").and_then(|v| v.as_str()).map(str::to_string));
        let artists = result
            .get("artists")
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|raw| serde_json::from_value::<YArtist>(raw.clone()).ok())
                    .map(|a| map_artist(&a))
                    .collect()
            })
            .unwrap_or_default();
        let mut tracks = Vec::new();
        if let Some(volumes) = result.get("volumes").and_then(|v| v.as_array()) {
            for volume in volumes {
                if let Some(arr) = volume.as_array() {
                    tracks.extend(map_tracks(arr));
                }
            }
        }
        Ok(AlbumPageDto {
            id: album_id.to_string(),
            title,
            cover_url: img(cover.as_deref(), "400x400"),
            year: result.get("year").and_then(|v| v.as_i64()),
            genre: result
                .get("genre")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            artists,
            tracks,
        })
    }

    pub async fn track(&self, track_id: &str) -> YResult<TrackDto> {
        let result = self.get_result(&format!("/tracks?track-ids={track_id}")).await?;
        let list: Vec<YTrack> =
            serde_json::from_value(result).map_err(|e| ApiError::Msg(e.to_string()))?;
        list.first()
            .map(map_track)
            .ok_or(ApiError::Msg("Трек не найден".to_string()))
    }

    /// Пачка треков по id (для лайков).
    pub async fn tracks(&self, ids: &[String]) -> YResult<Vec<TrackDto>> {
        let mut out = Vec::new();
        for chunk in ids.chunks(200) {
            let joined = chunk.join(",");
            let result = self
                .get_result(&format!("/tracks?track-ids={joined}"))
                .await?;
            let list: Vec<YTrack> =
                serde_json::from_value(result).map_err(|e| ApiError::Msg(e.to_string()))?;
            out.extend(list.iter().map(map_track));
        }
        Ok(out)
    }

    /// Колесо рекомендаций для главной.
    pub async fn wheel(&self) -> YResult<Vec<WheelItemDto>> {
        let value = self.post_json("/wheel/new").await?;
        let mut out = Vec::new();
        let items = match value.get("items").and_then(|i| i.as_array()) {
            Some(arr) => arr,
            None => return Ok(out),
        };
        for item in items {
            let kind_raw = item
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_uppercase();
            let id = item.get("id").map(val_to_id).unwrap_or_default();
            let data = item.get("data");
            let description = item
                .get("description")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            match kind_raw.as_str() {
                "WAVE" => {
                    let wave = data.and_then(|d| d.get("wave"));
                    let agent = data.and_then(|d| d.get("agent"));
                    let seed = wave
                        .and_then(|w| w.get("seeds"))
                        .and_then(|s| s.as_array())
                        .and_then(|a| a.first())
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                        .unwrap_or_else(|| id.clone());
                    out.push(WheelItemDto {
                        id: id.clone(),
                        kind: "wave".to_string(),
                        name: wave
                            .and_then(|w| w.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("Волна")
                            .to_string(),
                        description: wave
                            .and_then(|w| w.get("description"))
                            .and_then(|v| v.as_str())
                            .map(str::to_string)
                            .or(description),
                        cover_url: img(
                            agent.and_then(|a| str_at(a, &["cover", "uri"])).as_deref(),
                            "400x400",
                        ),
                        station: Some(seed),
                    });
                }
                "ALBUM" => {
                    let album = data.and_then(|d| d.get("album"));
                    out.push(WheelItemDto {
                        id: album
                            .and_then(|a| a.get("id"))
                            .map(val_to_id)
                            .unwrap_or_else(|| id.clone()),
                        kind: "album".to_string(),
                        name: album
                            .and_then(|a| a.get("title"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("Альбом")
                            .to_string(),
                        description,
                        cover_url: img(
                            album.and_then(|a| str_at(a, &["cover", "uri"])).as_deref(),
                            "400x400",
                        ),
                        station: None,
                    });
                }
                "ARTIST" => {
                    let artist = data.and_then(|d| d.get("artist"));
                    out.push(WheelItemDto {
                        id: artist
                            .and_then(|a| a.get("id"))
                            .map(val_to_id)
                            .unwrap_or_else(|| id.clone()),
                        kind: "artist".to_string(),
                        name: artist
                            .and_then(|a| a.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("Артист")
                            .to_string(),
                        description,
                        cover_url: img(
                            artist.and_then(|a| str_at(a, &["cover", "uri"])).as_deref(),
                            "400x400",
                        ),
                        station: None,
                    });
                }
                "PLAYLIST" => {
                    let pl = data.and_then(|d| d.get("playlist"));
                    out.push(WheelItemDto {
                        id: pl.and_then(|p| p.get("kind"))
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| id.clone()),
                        kind: "playlist".to_string(),
                        name: pl
                            .and_then(|p| p.get("title"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("Плейлист")
                            .to_string(),
                        description,
                        cover_url: img(
                            pl.and_then(|p| str_at(p, &["cover", "uri"])).as_deref(),
                            "400x400",
                        ),
                        station: None,
                    });
                }
                _ => {}
            }
        }
        Ok(out)
    }
}
