use super::*;

impl Yandex {
    /// старт или продолжение сессии волны. station: "user:onyourwave" = Моя волна.
    pub async fn wave_session(
        &self,
        station: &str,
        session_id: Option<&str>,
        queue: &[String],
    ) -> YResult<WaveResponse> {
        let (path, body) = match session_id {
            Some(sid) => (
                format!("/rotor/session/{sid}/tracks"),
                serde_json::json!({ "queue": queue }),
            ),
            None => (
                "/rotor/session/new".to_string(),
                serde_json::json!({ "seeds": [station], "includeTracksInResponse": true }),
            ),
        };
        let result = self.post_value(&path, body).await?;
        let parsed: YRotorSession =
            serde_json::from_value(result.clone()).map_err(|e| ApiError::Msg(e.to_string()))?;
        // derivedColors живёт в сыром JSON последовательности — маппим из него
        let sequence = result
            .get("sequence")
            .and_then(|s| s.as_array())
            .cloned()
            .unwrap_or_default();
        let tracks: Vec<TrackDto> = sequence
            .iter()
            .filter_map(|item| item.get("track"))
            .filter_map(|raw| serde_json::from_value::<YTrack>(raw.clone()).ok())
            .enumerate()
            .map(|(ix, t)| {
                let mut track = map_track(&t);
                if let Some(raw) = sequence.get(ix).and_then(|s| s.get("track")) {
                    crate::yandex::dto::set_derived_colors_pub(&mut track, raw);
                }
                track
            })
            .collect();
        Ok(WaveResponse {
            station_id: station.to_string(),
            radio_session_id: parsed.radio_session_id,
            batch_id: parsed.batch_id,
            tracks,
        })
    }

    pub async fn stations(&self) -> YResult<Vec<(String, String)>> {
        let result = self.get_result("/rotor/stations/dashboard").await?;
        let mut out = Vec::new();
        if let Some(arr) = result.get("stations").and_then(|s| s.as_array()) {
            for item in arr {
                let st = item.get("station").unwrap_or(item);
                let id_obj = st.get("id");
                let kind = id_obj.and_then(|i| i.get("type")).and_then(|v| v.as_str()).unwrap_or("");
                let tag = id_obj.and_then(|i| i.get("tag")).and_then(|v| v.as_str()).unwrap_or("");
                if kind.is_empty() || tag.is_empty() {
                    continue;
                }
                out.push((
                    format!("{kind}:{tag}"),
                    st.get("name").and_then(|v| v.as_str()).unwrap_or("Станция").to_string(),
                ));
            }
        }
        Ok(out)
    }

    pub async fn station_info(&self, station: &str) -> YResult<StationInfoDto> {
        let result = self
            .get_result(&format!("/rotor/station/{station}/info"))
            .await?;
        let value = match result {
            serde_json::Value::Array(items) => items
                .into_iter()
                .next()
                .ok_or_else(|| ApiError::Msg("пустой ответ станции".to_string()))?,
            other => other,
        };
        let station_obj = value.get("station").unwrap_or(&value);
        let settings = value
            .get("settings2")
            .or_else(|| value.get("settings"))
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let restrictions = station_obj
            .get("restrictions2")
            .or_else(|| station_obj.get("restrictions"))
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        let setting_value = |key: &str| -> Option<String> {
            settings.get(key).and_then(|v| v.as_str()).map(str::to_string)
        };
        let enum_options = |key: &str| -> Vec<StationOptionDto> {
            restrictions
                .get(key)
                .and_then(|v| v.get("possibleValues"))
                .and_then(|v| v.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| {
                            let value = item.get("value").and_then(|v| v.as_str())?;
                            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or(value);
                            Some(StationOptionDto {
                                value: value.to_string(),
                                name: name.to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default()
        };

        Ok(StationInfoDto {
            id: station.to_string(),
            name: station_obj
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(station)
                .to_string(),
            language: setting_value("language"),
            mood_energy: setting_value("moodEnergy"),
            languages: enum_options("language"),
            moods: enum_options("moodEnergy"),
        })
    }

    pub async fn set_station_settings(
        &self,
        station: &str,
        language: Option<&str>,
        mood_energy: Option<&str>,
    ) -> YResult<bool> {
        let mut body = serde_json::Map::new();
        if let Some(v) = language {
            body.insert("language".into(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = mood_energy {
            body.insert("moodEnergy".into(), serde_json::Value::String(v.to_string()));
        }
        if body.is_empty() {
            return Ok(false);
        }
        self.post_value(
            &format!("/rotor/station/{station}/settings3"),
            serde_json::Value::Object(body),
        )
        .await
        .map(|_| true)
    }

    /// Обратная связь волны: play/skipForward/skipBack…
    pub async fn feedback(
        &self,
        station: &str,
        kind: &str,
        track_id: Option<&str>,
        batch_id: Option<&str>,
        total_seconds: Option<f64>,
    ) -> YResult<bool> {
        let path = match batch_id {
            Some(b) => format!("/rotor/station/{station}/feedback?batch-id={b}"),
            None => format!("/rotor/station/{station}/feedback"),
        };
        let mut body = serde_json::json!({ "type": kind, "from": "meowsic" });
        if let Some(t) = track_id {
            body["trackId"] = serde_json::json!(t);
        }
        if let Some(s) = total_seconds {
            body["totalPlayedSeconds"] = serde_json::json!(s);
        }
        self.post_value(&path, body).await.map(|_| true)
    }
}
