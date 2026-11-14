use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use gpui_kit::prelude::*;
use gpui_kit::{Context, Entity, EventEmitter, SharedString};

use crate::store::Store;
use crate::yandex::{ProfileDto, TokenPoll, Yandex};

#[derive(Clone, Debug)]
pub enum SessionEvent {
    LoggedIn,
    LoggedOut,
    LoginFailed(SharedString),
    OauthChanged,
}

#[derive(Clone, Debug)]
pub enum OauthState {
    Idle,
    Waiting { user_code: SharedString, url: SharedString },
    Failed(SharedString),
}

/// Открыть ссылку в системном браузере — как делает mashiro.
pub fn open_in_browser(url: &str) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

/// Токен и профиль. Держит общий клиент Яндекса: все, кому нужен API,
/// клонируют снимок через [`Session::yandex`].
pub struct Session {
    api: Arc<Mutex<Yandex>>,
    profile: Option<ProfileDto>,
    oauth: OauthState,
    oauth_gen: Arc<AtomicU64>,
}

impl EventEmitter<SessionEvent> for Session {}

impl Session {
    pub fn new(store: &Entity<Store>, cx: &mut Context<Self>) -> Self {
        let token = store.read(cx).settings.token.clone();
        Self {
            api: Arc::new(Mutex::new(Yandex::new(&token))),
            profile: None,
            oauth: OauthState::Idle,
            oauth_gen: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn oauth(&self) -> &OauthState {
        &self.oauth
    }

    /// Device OAuth: запрос кода → браузер → опрос токена → обычный login.
    pub fn start_oauth(&mut self, cx: &mut Context<Self>) {
        let api = self.yandex();
        let gen = self.oauth_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let gen_arc = Arc::clone(&self.oauth_gen);
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let dc = match api.oauth_device_code().await {
                Ok(dc) => dc,
                Err(e) => {
                    let _ = weak.update(cx, |this, cx| {
                        this.oauth = OauthState::Failed(e.to_string().into());
                        cx.emit(SessionEvent::OauthChanged);
                    });
                    return;
                }
            };
            open_in_browser(&dc.verification_url);
            let device_code = dc.device_code.clone();
            let user_code: SharedString = dc.user_code.clone().into();
            let url: SharedString = dc.verification_url.clone().into();
            let mut interval = dc.interval;
            let expires = dc.expires_in;
            let _ = weak.update(cx, |this, cx| {
                this.oauth = OauthState::Waiting { user_code, url };
                cx.emit(SessionEvent::OauthChanged);
            });

            let mut waited: u64 = 0;
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(interval))
                    .await;
                waited += interval;
                if gen_arc.load(Ordering::SeqCst) != gen {
                    return; // отменено или перезапущено
                }
                if waited >= expires {
                    let _ = weak.update(cx, |this, cx| {
                        this.oauth =
                            OauthState::Failed("Время ожидания истекло. Попробуй снова.".into());
                        cx.emit(SessionEvent::OauthChanged);
                    });
                    return;
                }
                match api.oauth_poll_token(&device_code).await {
                    Ok(TokenPoll::Token(token)) => {
                        let _ = weak.update(cx, |this, cx| {
                            this.oauth = OauthState::Idle;
                            cx.emit(SessionEvent::OauthChanged);
                            this.login(token, cx);
                        });
                        return;
                    }
                    Ok(TokenPoll::Pending) => {}
                    Ok(TokenPoll::SlowDown) => interval += 5,
                    Ok(TokenPoll::Failed(desc)) => {
                        let msg = desc.into();
                        let _ = weak.update(cx, |this, cx| {
                            this.oauth = OauthState::Failed(msg);
                            cx.emit(SessionEvent::OauthChanged);
                        });
                        return;
                    }
                    Err(e) => {
                        let msg = e.to_string().into();
                        let _ = weak.update(cx, |this, cx| {
                            this.oauth = OauthState::Failed(msg);
                            cx.emit(SessionEvent::OauthChanged);
                        });
                        return;
                    }
                }
            }
        })
        .detach();
    }

    pub fn cancel_oauth(&mut self, cx: &mut Context<Self>) {
        self.oauth_gen.fetch_add(1, Ordering::SeqCst);
        self.oauth = OauthState::Idle;
        cx.emit(SessionEvent::OauthChanged);
    }

    pub fn reopen_oauth_page(&self) {
        if let OauthState::Waiting { url, .. } = &self.oauth {
            open_in_browser(url.as_ref());
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
