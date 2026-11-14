use std::collections::HashSet;

use gpui_kit::prelude::*;
use gpui_kit::{Context, EventEmitter, SharedString};

use crate::yandex::{PlaylistDto, Yandex};

#[derive(Clone, Debug)]
pub enum LibEvent {
    Changed,
    Error(SharedString),
}

/// Локальный индекс библиотеки: лайки/дизлайки и список плейлистов.
/// Нужен, чтобы иконки в строках не дёргали сеть на каждый рендер.
pub struct LibIndex {
    pub liked: HashSet<String>,
    pub disliked: HashSet<String>,
    pub playlists: Vec<PlaylistDto>,
    uid: i64,
}

impl EventEmitter<LibEvent> for LibIndex {}

impl LibIndex {
    pub fn new() -> Self {
        Self {
            liked: HashSet::new(),
            disliked: HashSet::new(),
            playlists: Vec::new(),
            uid: 0,
        }
    }

    pub fn is_liked(&self, id: &str) -> bool {
        self.liked.contains(id)
    }

    pub fn is_disliked(&self, id: &str) -> bool {
        self.disliked.contains(id)
    }

    pub fn playlists(&self) -> &[PlaylistDto] {
        &self.playlists
    }

    pub fn set_uid(&mut self, uid: i64, cx: &mut Context<Self>) {
        self.uid = uid;
        if uid == 0 {
            self.liked.clear();
            self.disliked.clear();
            self.playlists.clear();
            cx.emit(LibEvent::Changed);
        }
    }

    /// Полная перезагрузка индекса.
    pub fn refresh(&mut self, api: &Yandex, cx: &mut Context<Self>) {
        if self.uid == 0 || !api.has_token() {
            return;
        }
        let api = api.clone();
        let uid = self.uid;
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let liked = api.liked_ids(uid).await;
            let disliked = api.disliked_ids(uid).await;
            let playlists = api.playlists(uid).await;
            let _ = weak.update(cx, |this, cx| match liked {
                Ok(liked) => {
                    this.liked = liked.into_iter().collect();
                    this.disliked = disliked.unwrap_or_default().into_iter().collect();
                    if let Ok(list) = playlists {
                        this.playlists = list;
                    }
                    cx.emit(LibEvent::Changed);
                }
                Err(e) => cx.emit(LibEvent::Error(e.to_string().into())),
            });
        })
        .detach();
    }

    /// Переключение лайка/дизлайка с оптимистичным обновлением множества.
    pub fn toggle_like(
        &mut self,
        api: &Yandex,
        track_id: &str,
        dislike: bool,
        cx: &mut Context<Self>,
    ) {
        if self.uid == 0 {
            return;
        }
        let remove = if dislike {
            self.disliked.remove(track_id)
        } else {
            self.liked.remove(track_id)
        };
        if !remove {
            if dislike {
                self.disliked.insert(track_id.to_string());
                self.liked.remove(track_id);
            } else {
                self.liked.insert(track_id.to_string());
                self.disliked.remove(track_id);
            }
        }
        cx.emit(LibEvent::Changed);

        let api = api.clone();
        let uid = self.uid;
        let id = track_id.to_string();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let res = api.set_like(uid, &id, dislike, remove).await;
            if let Err(e) = res {
                let _ = weak.update(cx, |this, cx| {
                    // откатываем оптимистичное изменение
                    if remove {
                        if dislike {
                            this.disliked.insert(id.clone());
                        } else {
                            this.liked.insert(id.clone());
                        }
                    } else if dislike {
                        this.disliked.remove(&id);
                    } else {
                        this.liked.remove(&id);
                    }
                    cx.emit(LibEvent::Changed);
                    cx.emit(LibEvent::Error(e.to_string().into()));
                });
            }
        })
        .detach();
    }

    pub fn report_error(&mut self, msg: &str, cx: &mut Context<Self>) {
        cx.emit(LibEvent::Error(msg.to_string().into()));
    }

    pub fn replace_playlists(&mut self, list: Vec<PlaylistDto>, cx: &mut Context<Self>) {
        self.playlists = list;
        cx.emit(LibEvent::Changed);
    }
}
