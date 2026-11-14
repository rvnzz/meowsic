use std::sync::{Arc, Mutex};

use gpui_kit::prelude::*;
use gpui_kit::{Context, Entity, EventEmitter, SharedString};

use crate::store::Store;
use crate::yandex::{ProfileDto, Yandex};

#[derive(Clone, Debug)]
pub enum SessionEvent {
    LoggedIn,
    LoggedOut,
    LoginFailed(SharedString),
}

/// Токен и профиль. Держит общий клиент Яндекса: все, кому нужен API,
/// клонируют снимок через [`Session::yandex`].
pub struct Session {
    api: Arc<Mutex<Yandex>>,
    profile: Option<ProfileDto>,
}

impl EventEmitter<SessionEvent> for Session {}

impl Session {
    pub fn new(store: &Entity<Store>, cx: &mut Context<Self>) -> Self {
        let token = store.read(cx).settings.token.clone();
        Self {
            api: Arc::new(Mutex::new(Yandex::new(&token))),
            profile: None,
        }
    }

    pub fn api_handle(&self) -> Arc<Mutex<Yandex>> {
        Arc::clone(&self.api)
    }

    pub fn yandex(&self) -> Yandex {
        self.api.lock().unwrap().clone()
    }

    pub fn is_logged_in(&self) -> bool {
        self.profile.is_some()
    }

    pub fn profile(&self) -> Option<&ProfileDto> {
        self.profile.as_ref()
    }

    /// Проверяет токен в сети; результат приносит событие.
    pub fn login(&mut self, token: String, cx: &mut Context<Self>) {
        let candidate = Yandex::new(&token);
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            match candidate.account_status().await {
                Ok((profile, _uid)) => {
                    let _ = weak.update(cx, |this, cx| {
                        this.api.lock().unwrap().set_token(&token);
                        this.profile = Some(profile);
                        cx.emit(SessionEvent::LoggedIn);
                    });
                }
                Err(e) => {
                    let _ = weak.update(cx, |_this, cx| {
                        cx.emit(SessionEvent::LoginFailed(e.to_string().into()));
                    });
                }
            }
        })
        .detach();
    }

    pub fn logout(&mut self, cx: &mut Context<Self>) {
        self.profile = None;
        self.api.lock().unwrap().set_token("");
        cx.emit(SessionEvent::LoggedOut);
    }
}
