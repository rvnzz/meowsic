use super::*;

impl Yandex {
    pub async fn set_like(&self, uid: i64, track_id: &str, dislike: bool, remove: bool) -> YResult<bool> {
        if uid <= 0 {
            return Err(ApiError::Msg("Нет данных аккаунта, войди заново".to_string()));
        }
        let bucket = if dislike { "dislikes" } else { "likes" };
        let action = if remove { "remove" } else { "add-multiple" };
        let path = format!("/users/{uid}/{bucket}/tracks/{action}");
        let body = format!("track-ids={}", urlencoding::encode(track_id));
        let status = self.post_status(&path, "application/x-www-form-urlencoded", body.clone()).await?;
        if status < 300 {
            return Ok(true);
        }
        if !remove {
            // старый эндпоинт у некоторых аккаунтов жив
            let legacy = format!("/users/{uid}/{bucket}/tracks/add");
            let retry = self.post_status(&legacy, "application/x-www-form-urlencoded", body).await?;
            if retry < 300 {
                return Ok(true);
            }
        }
        Err(ApiError::Http(status))
    }

    pub async fn liked_ids(&self, uid: i64) -> YResult<Vec<String>> {
        let result = self.get_result(&format!("/users/{uid}/likes/tracks")).await?;
        Ok(result
            .get("library")
            .and_then(|l| l.get("tracks"))
            .and_then(|t| t.as_array())
            .map(|arr| arr.iter().filter_map(|x| x.get("id")).map(val_to_id).collect())
            .unwrap_or_default())
    }

    pub async fn disliked_ids(&self, uid: i64) -> YResult<Vec<String>> {
        let result = self.get_result(&format!("/users/{uid}/dislikes/tracks")).await?;
        Ok(result
            .get("library")
            .and_then(|l| l.get("tracks"))
            .and_then(|t| t.as_array())
            .map(|arr| arr.iter().filter_map(|x| x.get("id")).map(val_to_id).collect())
            .unwrap_or_default())
    }

    pub async fn playlists(&self, uid: i64) -> YResult<Vec<PlaylistDto>> {
        let result = self
            .get_result(&format!("/users/{uid}/playlists/list"))
            .await?;
        let raw: Vec<YPlaylist> =
            serde_json::from_value(result).map_err(|e| ApiError::Msg(e.to_string()))?;
        Ok(raw
            .into_iter()
            .map(|p| PlaylistDto {
                kind: p.kind,
                title: p.title.unwrap_or_else(|| "Без названия".to_string()),
                track_count: p.track_count.unwrap_or(0),
                cover_url: img(p.cover.as_ref().and_then(|c| c.uri.clone()).or(p.og_image).as_deref(), "200x200"),
                owner_uid: p.owner.and_then(|o| o.uid),
            })
            .collect())
    }

    pub async fn playlist(&self, uid: i64, kind: i64) -> YResult<PlaylistFullDto> {
        let result = self
            .get_result(&format!("/users/{uid}/playlists/{kind}"))
            .await?;
        let title = result
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Плейлист")
            .to_string();
        let revision = result.get("revision").and_then(|v| v.as_i64()).unwrap_or(0);
        let full: YPlaylistFull =
            serde_json::from_value(result).map_err(|e| ApiError::Msg(e.to_string()))?;
        Ok(PlaylistFullDto {
            kind,
            title,
            revision,
            tracks: full.tracks.iter().filter_map(|w| w.track.as_ref()).map(map_track).collect(),
        })
    }

    pub async fn playlist_create(&self, uid: i64, title: &str, public: bool) -> YResult<PlaylistDto> {
        let title = title.trim();
        if title.is_empty() {
            return Err(ApiError::Msg("Укажи название плейлиста".to_string()));
        }
        let visibility = if public { "public" } else { "private" };
        let result = self
            .post_form(
                &format!("/users/{uid}/playlists/create"),
                &[("title", title.to_string()), ("visibility", visibility.to_string())],
            )
            .await?;
        let created: YPlaylist =
            serde_json::from_value(result).map_err(|e| ApiError::Msg(e.to_string()))?;
        Ok(PlaylistDto {
            kind: created.kind,
            title: created.title.unwrap_or_else(|| title.to_string()),
            track_count: created.track_count.unwrap_or(0),
            cover_url: img(created.cover.as_ref().and_then(|c| c.uri.clone()).or(created.og_image).as_deref(), "200x200"),
            owner_uid: created.owner.and_then(|o| o.uid),
        })
    }

    pub async fn playlist_rename(&self, uid: i64, kind: i64, title: &str) -> YResult<bool> {
        self.post_form(
            &format!("/users/{uid}/playlists/{kind}/name"),
            &[("value", title.trim().to_string())],
        )
        .await
        .map(|_| true)
    }

    pub async fn playlist_remove(&self, uid: i64, kind: i64) -> YResult<bool> {
        self.post_form(&format!("/users/{uid}/playlists/{kind}/delete"), &[])
            .await
            .map(|_| true)
    }

    /// JSON-patch diff поверх revision; при 412 притупляет один раз.
    async fn change_relative(&self, uid: i64, kind: i64, diff: String) -> YResult<bool> {
        for _ in 0..2 {
            let full = self.playlist(uid, kind).await?;
            let path = format!("/users/{uid}/playlists/{kind}/change-relative");
            let body = format!(
                "diff={}&revision={}",
                urlencoding::encode(&diff),
                full.revision
            );
            let status = self.post_status(&path, "application/x-www-form-urlencoded", body).await?;
            if status < 300 {
                return Ok(true);
            }
            if status != 412 && status != 409 {
                return Err(ApiError::Http(status));
            }
        }
        Err(ApiError::Msg("Плейлист изменился на сервере, обнови его".to_string()))
    }

    pub async fn playlist_insert(
        &self,
        uid: i64,
        kind: i64,
        at: i64,
        track_id: &str,
        album_id: &str,
    ) -> YResult<bool> {
        let diff = serde_json::json!([{
            "op": "insert",
            "at": at,
            "tracks": [{ "id": track_id, "albumId": album_id }],
        }]);
        self.change_relative(uid, kind, diff.to_string()).await
    }

    pub async fn playlist_delete_track(
        &self,
        uid: i64,
        kind: i64,
        from: i64,
        track_id: &str,
        album_id: &str,
    ) -> YResult<bool> {
        let diff = serde_json::json!([{
            "op": "delete",
            "from": from,
            "to": from + 1,
            "tracks": [{ "id": track_id, "albumId": album_id }],
        }]);
        self.change_relative(uid, kind, diff.to_string()).await
    }

    /// Лайкнутые треки целиком.
    pub async fn liked_tracks(&self, uid: i64) -> YResult<Vec<TrackDto>> {
        let ids = self.liked_ids(uid).await?;
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.tracks(&ids).await
    }
}
