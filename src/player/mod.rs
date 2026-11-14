mod eq;

use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
    sync::{Arc, Mutex},
    time::Duration,
};

use gpui_kit::prelude::*;
use gpui_kit::{Context, Entity, EventEmitter, SharedString, Task, WeakEntity};
use rodio::{DeviceSinkBuilder, MixerDeviceSink};

use crate::store::{Quality, SavedSession, Store};
use crate::yandex::{TrackDto, WaveResponse, Yandex};

use eq::EqSource;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatMode {
    Off,
    All,
    One,
}

impl RepeatMode {
    pub fn next(self) -> Self {
        match self {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            RepeatMode::Off => "Без повтора",
            RepeatMode::All => "Повтор очереди",
            RepeatMode::One => "Повтор трека",
        }
    }
}

#[derive(Clone, Debug)]
pub enum PlayerEvent {
    State,
}

#[derive(Clone)]
pub struct WaveCtx {
    pub station: String,
    pub name: SharedString,
    pub session_id: Option<String>,
    pub batch_id: Option<String>,
}

/// Трек в очереди для отображения.
pub struct QueueItem {
    pub track: TrackDto,
    pub in_queue_ix: usize,
}

impl EventEmitter<PlayerEvent> for Player {}

pub struct Player {
    yandex: Arc<Mutex<Yandex>>,
    store: Entity<Store>,

    // воспроизведение
    sink: Option<MixerDeviceSink>,
    rodio: Option<rodio::Player>,
    pub playing: bool,
    pub loading: bool,
    pub error: Option<SharedString>,

    // очередь
    pub queue: Vec<TrackDto>,
    order: Vec<usize>,
    order_pos: usize,
    pub current: Option<TrackDto>,
    pub context_name: SharedString,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub wave: Option<WaveCtx>,

    // настройки звука
    volume: f32,
    quality: Quality,
    eq_enabled: bool,
    eq_gains: Vec<f32>,

    // кеш скачанного: id → тело трека
    downloads: HashMap<String, Arc<Vec<u8>>>,
    inflight: HashSet<String>,
    wave_refresh: bool,
    gen: usize,

    started: bool,
    tick: Option<Task<()>>,
    sleep_task: Task<()>,
    last_session_save: std::time::Instant,
    pub uid: i64,
}

impl Player {
    pub fn new(yandex: Arc<Mutex<Yandex>>, store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        let settings = store.read(cx).settings.clone();
        let mut player = Self {
            yandex,
            store,
            sink: None,
            rodio: None,
            playing: false,
            loading: false,
            error: None,
            queue: Vec::new(),
            order: Vec::new(),
            order_pos: 0,
            current: None,
            context_name: "".into(),
            shuffle: false,
            repeat: RepeatMode::All,
            wave: None,
            volume: settings.volume,
            quality: settings.quality,
            eq_enabled: settings.eq.enabled,
            eq_gains: settings.eq.gains_db.clone(),
            downloads: HashMap::new(),
            inflight: HashSet::new(),
            wave_refresh: false,
            gen: 0,
            started: false,
            tick: None,
            sleep_task: Task::ready(()),
            last_session_save: std::time::Instant::now(),
            uid: 0,
        };
        player.ensure_ticker(cx);
        player
    }

    fn yandex(&self) -> Yandex {
        self.yandex.lock().unwrap().clone()
    }

    fn ensure_ticker(&mut self, cx: &mut Context<Self>) {
        if self.tick.is_some() {
            return;
        }
        let weak: WeakEntity<Self> = cx.entity().downgrade();
        self.tick = Some(cx.spawn(async move |_, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(250)).await;
                let alive = weak
                    .update(cx, |this, cx| {
                        this.on_tick(cx);
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        }));
    }

    fn on_tick(&mut self, cx: &mut Context<Self>) {
        let Some(rodio) = self.rodio.as_ref() else {
            return;
        };
        if self.loading || !self.playing {
            return;
        }
        if self.started && rodio.empty() {
            // трек доиграл
            self.on_track_end(cx);
            return;
        }
        // волна: подтягиваем следующую пачку заранее
        if self.wave.is_some()
            && !self.wave_refresh
            && self.order_pos + 2 >= self.order.len()
            && !self.order.is_empty()
        {
            self.wave_more(cx);
        }
        if self.last_session_save.elapsed() > Duration::from_secs(10) {
            self.save_session_now(cx);
        }
        cx.emit(PlayerEvent::State);
    }

    fn on_track_end(&mut self, cx: &mut Context<Self>) {
        if self.repeat == RepeatMode::One {
            if let Some(track) = self.current.clone() {
                let bytes = self.downloads.get(&track.id).cloned();
                if let Some(bytes) = bytes {
                    self.start_from_bytes(&bytes, 0, true, cx);
                    return;
                }
            }
        }
        self.advance(1, false, cx);
    }

    // --- публичный API ---

    pub fn duration_ms(&self) -> i64 {
        self.current
            .as_ref()
            .and_then(|t| t.duration_ms)
            .unwrap_or(0)
    }

    pub fn position_ms(&self) -> i64 {
        if let Some(rodio) = self.rodio.as_ref() {
            rodio.get_pos().as_millis() as i64
        } else {
            0
        }
    }

    pub fn is_active(&self) -> bool {
        self.current.is_some()
    }

    pub fn snapshot_queue(&self) -> Vec<QueueItem> {
        self.order
            .iter()
            .enumerate()
            .map(|(_ix, &qix)| QueueItem {
                track: self.queue[qix].clone(),
                in_queue_ix: qix,
            })
            .collect()
    }

    pub fn current_order_ix(&self) -> usize {
        self.order_pos
    }

    /// Обычный контекст: альбом, плейлист, лайки…
    pub fn play_context(&mut self, name: &str, tracks: Vec<TrackDto>, start_ix: usize, cx: &mut Context<Self>) {
        if tracks.is_empty() {
            return;
        }
        self.wave = None;
        self.context_name = name.into();
        self.queue = tracks;
        self.rebuild_order(start_ix, cx);
    }

    fn rebuild_order(&mut self, play_at_queue_ix: usize, cx: &mut Context<Self>) {
        self.order = (0..self.queue.len()).collect();
        if self.shuffle {
            let mut rng = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(1))
                .to_le_bytes();
            // простой xorshift перемешиванием Fisher–Yates
            let mut seed = u64::from_le_bytes(rng);
            for ix in (1..self.order.len()).rev() {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let j = (seed as usize) % (ix + 1);
                self.order.swap(ix, j);
            }
            rng = seed.to_le_bytes();
            let _ = rng;
        }
        self.order_pos = self
            .order
            .iter()
            .position(|&q| q == play_at_queue_ix)
            .unwrap_or(0);
        let at = self.order[self.order_pos];
        let track = self.queue[at].clone();
        self.begin_track(&track, 0, true, cx);
    }

    pub fn start_wave(&mut self, station: &str, name: &str, cx: &mut Context<Self>) {
        let api = self.yandex();
        let station = station.to_string();
        let name: SharedString = name.into();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let res = api.wave_session(&station, None, &[]).await;
            let _ = weak.update(cx, |this, cx| match res {
                Ok(wave) => this.apply_wave(wave, name, cx),
                Err(e) => {
                    this.error = Some(e.to_string().into());
                    cx.emit(PlayerEvent::State);
                }
            });
        })
        .detach();
    }

    pub fn apply_wave(&mut self, wave: WaveResponse, name: SharedString, cx: &mut Context<Self>) {
        self.context_name = name.clone();
        self.wave = Some(WaveCtx {
            station: wave.station_id.clone(),
            name,
            session_id: wave.radio_session_id,
            batch_id: wave.batch_id,
        });
        self.queue = wave.tracks;
        self.rebuild_order(0, cx);
    }

    fn wave_more(&mut self, cx: &mut Context<Self>) {
        let Some(wave) = self.wave.clone() else {
            return;
        };
        let api = self.yandex();
        self.wave_refresh = true;
        let queue_ids: Vec<String> = self.order.iter().map(|&ix| self.queue[ix].id.clone()).collect();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let res = api.wave_session(&wave.station, wave.session_id.as_deref(), &queue_ids).await;
            let _ = weak.update(cx, |this, cx| {
                this.wave_refresh = false;
                if let Ok(more) = res {
                    let mut fresh: Vec<TrackDto> = Vec::new();
                    for t in more.tracks {
                        if !this.queue.iter().any(|q| q.id == t.id) {
                            fresh.push(t);
                        }
                    }
                    if let Some(w) = this.wave.as_mut() {
                        w.batch_id = more.batch_id.or(w.batch_id.clone());
                    }
                    let base = this.queue.len();
                    for (ix, t) in fresh.into_iter().enumerate() {
                        this.queue.push(t);
                        if this.shuffle {
                            // вшафленную добавку вставляем в случайную позицию хвоста
                            let tail = this.order.len();
                            let at = tail - (tail - base).min(1);
                            this.order.insert(at.max(this.order_pos + 1), base + ix);
                        } else {
                            this.order.push(base + ix);
                        }
                    }
                    cx.emit(PlayerEvent::State);
                }
            });
        })
        .detach();
    }

    pub fn wave_feedback_pub(&self, kind: &str, cx: &mut Context<Self>) {
        let Some(wave) = self.wave.as_ref() else {
            return;
        };
        let Some(track) = self.current.as_ref() else {
            return;
        };
        let api = self.yandex();
        let station = wave.station.clone();
        let track_id = track.id.clone();
        let batch = wave.batch_id.clone();
        let total = self.position_ms() as f64 / 1000.0;
        let kind = kind.to_string();
        cx.spawn(async move |_, _| {
            let _ = api.feedback(&station, &kind, Some(&track_id), batch.as_deref(), Some(total)).await;
        })
        .detach();
    }

    /// Переключение перемешивания без потери текущего трека.
    pub fn set_shuffle(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.shuffle == on {
            return;
        }
        self.shuffle = on;
        let current_ix = self.order.get(self.order_pos).copied();
        self.order = (0..self.queue.len()).collect();
        if on {
            let mut seed = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(7))
                | 1;
            for ix in (1..self.order.len()).rev() {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let j = (seed as usize) % (ix + 1);
                self.order.swap(ix, j);
            }
        }
        if let Some(qix) = current_ix {
            self.order_pos = self.order.iter().position(|&q| q == qix).unwrap_or(0);
        }
        self.save_session_now(cx);
        cx.emit(PlayerEvent::State);
    }

    pub fn set_repeat(&mut self, mode: RepeatMode, cx: &mut Context<Self>) {
        self.repeat = mode;
        if let Some(rodio) = self.rodio.as_ref() {
            rodio.set_volume(self.volume.max(0.0));
        }
        self.save_session_now(cx);
        cx.emit(PlayerEvent::State);
    }

    pub fn cycle_repeat(&mut self, cx: &mut Context<Self>) {
        let next = self.repeat.next();
        self.set_repeat(next, cx);
    }

    pub fn next(&mut self, cx: &mut Context<Self>) {
        self.advance(1, true, cx);
    }

    pub fn prev(&mut self, cx: &mut Context<Self>) {
        // как в нормальных плеерах: сначала на начало текущего
        if self.position_ms() > 3000 {
            self.seek(0, cx);
            return;
        }
        self.advance(-1, true, cx);
    }

    fn advance(&mut self, dir: i32, user: bool, cx: &mut Context<Self>) {
        if self.order.is_empty() {
            return;
        }
        if user && self.wave.is_some() {
            let kind = if dir > 0 { "skipForward" } else { "skipBack" };
            self.wave_feedback_pub(kind, cx);
        }
        let len = self.order.len() as i32;
        let mut next = self.order_pos as i32 + dir;
        if next >= len {
            if self.wave.is_some() {
                next = len - 1;
                self.wave_more(cx);
                // волна бесконечна: подождём пачку и продолжим по тикеру
                self.playing = false;
                if let Some(rodio) = self.rodio.as_ref() {
                    rodio.stop();
                }
                self.started = false;
                cx.emit(PlayerEvent::State);
                return;
            }
            match self.repeat {
                RepeatMode::Off => {
                    self.stop_internal(cx);
                    return;
                }
                RepeatMode::All | RepeatMode::One => next = 0,
            }
        }
        if next < 0 {
            next = if self.repeat == RepeatMode::All { len - 1 } else { 0 };
        }
        self.order_pos = next as usize;
        let track = self.queue[self.order[self.order_pos]].clone();
        self.begin_track(&track, 0, true, cx);
    }

    pub fn play_at_order_ix(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix >= self.order.len() {
            return;
        }
        self.order_pos = ix;
        let track = self.queue[self.order[ix]].clone();
        self.begin_track(&track, 0, true, cx);
    }

    pub fn toggle_play(&mut self, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        let Some(rodio) = self.rodio.as_ref() else {
            // старт с нуля: если волна — новый заход на станцию
            if let Some(wave) = self.wave.clone() {
                self.start_wave(&wave.station, &wave.name, cx);
            }
            return;
        };
        if self.playing {
            rodio.pause();
            self.playing = false;
        } else {
            rodio.play();
            self.playing = true;
        }
        self.save_session_now(cx);
        cx.emit(PlayerEvent::State);
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        self.save_session_now(cx);
        self.stop_internal(cx);
    }

    fn stop_internal(&mut self, cx: &mut Context<Self>) {
        if let Some(rodio) = self.rodio.as_ref() {
            rodio.stop();
        }
        self.playing = false;
        self.started = false;
        self.loading = false;
        self.current = None;
        cx.emit(PlayerEvent::State);
    }

    pub fn seek(&mut self, ms: i64, cx: &mut Context<Self>) {
        if self.current.is_none() {
            return;
        }
        let Some(rodio) = self.rodio.as_ref() else {
            return;
        };
        let _ = rodio.try_seek(Duration::from_millis(ms.max(0) as u64));
        cx.emit(PlayerEvent::State);
    }

    pub fn set_volume(&mut self, v: f32, cx: &mut Context<Self>) {
        self.volume = v.clamp(0.0, 1.0);
        if let Some(rodio) = self.rodio.as_ref() {
            rodio.set_volume(self.volume);
        }
        cx.emit(PlayerEvent::State);
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn set_quality(&mut self, q: Quality, cx: &mut Context<Self>) {
        if self.quality == q {
            return;
        }
        self.quality = q;
        self.downloads.clear();
        cx.emit(PlayerEvent::State);
    }

    pub fn set_eq(&mut self, enabled: bool, gains: Vec<f32>, cx: &mut Context<Self>) {
        self.eq_enabled = enabled;
        self.eq_gains = gains;
        // цепь перестраивается перезапуском трека с той же позиции
        if let Some(track) = self.current.clone() {
            if let Some(bytes) = self.downloads.get(&track.id).cloned() {
                let pos = self.position_ms();
                let autoplay = self.playing;
                self.start_from_bytes(&bytes, pos, autoplay, cx);
                return;
            }
        }
        cx.emit(PlayerEvent::State);
    }

    pub fn set_uid(&mut self, uid: i64) {
        self.uid = uid;
    }

    /// Сон: остановить воспроизведение через minutes минут.
    pub fn start_sleep_timer(&mut self, minutes: u64, cx: &mut Context<Self>) {
        self.sleep_task = Task::ready(());
        if minutes == 0 {
            return;
        }
        let weak = cx.entity().downgrade();
        self.sleep_task = cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(Duration::from_secs(minutes * 60))
                .await;
            let _ = weak.update(cx, |this, cx| {
                this.stop(cx);
                this.error = Some("Таймер сна: воспроизведение остановлено".into());
            });
        });
    }

    pub fn cancel_sleep_timer(&mut self) {
        self.sleep_task = Task::ready(());
    }

    // --- загрузка и запуск ---

    fn begin_track(&mut self, track: &TrackDto, from_ms: i64, autoplay: bool, cx: &mut Context<Self>) {
        self.gen += 1;
        let gen = self.gen;
        self.error = None;
        self.current = Some(track.clone());
        self.started = false;
        self.playing = false;

        if let Some(bytes) = self.downloads.get(&track.id).cloned() {
            self.apply_bytes(gen, &bytes, from_ms, autoplay, cx);
        } else {
            self.loading = true;
            cx.emit(PlayerEvent::State);
            self.request_download(track.clone(), gen, from_ms, autoplay, cx);
        }
        self.prefetch_next(cx);
        self.save_session_now(cx);
    }

    fn apply_bytes(&mut self, gen: usize, bytes: &Arc<Vec<u8>>, from_ms: i64, autoplay: bool, cx: &mut Context<Self>) {
        if gen != self.gen {
            return;
        }
        self.loading = false;
        self.start_from_bytes(bytes, from_ms, autoplay, cx);
    }

    fn start_from_bytes(&mut self, bytes: &Arc<Vec<u8>>, from_ms: i64, autoplay: bool, cx: &mut Context<Self>) {
        if let Err(e) = self.ensure_sink(cx) {
            self.error = Some(e.into());
            cx.emit(PlayerEvent::State);
            return;
        }
        let Some(track) = self.current.clone() else { return };
        self.downloads.insert(track.id.clone(), Arc::clone(bytes));

        let data = Cursor::new(bytes.as_ref().clone());
        let decoder = match rodio::Decoder::new(data) {
            Ok(d) => d,
            Err(e) => {
                self.error = Some(format!("Не удалось раскодировать трек: {e}").into());
                cx.emit(PlayerEvent::State);
                return;
            }
        };

        let use_eq = self.eq_enabled && self.eq_gains.iter().any(|g| g.abs() > 0.01);
        let rodio = self.rodio.as_ref().expect("rodio just created");
        rodio.stop();
        if use_eq {
            let gains = self.eq_gains.clone();
            let src = EqSource::new(decoder, &crate::store::EQ_FREQUENCIES, &gains);
            rodio.append(src);
        } else {
            rodio.append(decoder);
        }
        rodio.set_volume(self.volume);
        if from_ms > 0 {
            let _ = rodio.try_seek(Duration::from_millis(from_ms as u64));
        }
        if autoplay {
            rodio.play();
            self.playing = true;
        } else {
            rodio.pause();
            self.playing = false;
        }
        self.started = true;
        self.loading = false;
        cx.emit(PlayerEvent::State);
    }

    fn ensure_sink(&mut self, _cx: &mut Context<Self>) -> Result<(), String> {
        if self.sink.is_some() {
            return Ok(());
        }
        let mut sink = DeviceSinkBuilder::open_default_sink().map_err(|e| format!("Аудио-устройство: {e}"))?;
        sink.log_on_drop(false);
        let rodio = rodio::Player::connect_new(sink.mixer());
        self.sink = Some(sink);
        self.rodio = Some(rodio);
        Ok(())
    }

    fn request_download(&mut self, track: TrackDto, gen: usize, from_ms: i64, autoplay: bool, cx: &mut Context<Self>) {
        if !self.inflight.insert(track.id.clone()) {
            return;
        }
        let api = self.yandex();
        let quality = self.quality;
        let id = track.id.clone();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let res = async {
                let stream = api.stream(&id, quality).await?;
                api.download(&stream).await
            }
            .await;
            let _ = weak.update(cx, |this, cx| {
                this.inflight.remove(&id);
                match res {
                    Ok(bytes) => {
                        this.downloads.insert(id.clone(), Arc::clone(&bytes));
                        this.apply_bytes(gen, &bytes, from_ms, autoplay, cx);
                        this.prime_downloads();
                    }
                    Err(e) => {
                        if gen == this.gen {
                            this.loading = false;
                            this.error = Some(e.to_string().into());
                            cx.emit(PlayerEvent::State);
                        }
                    }
                }
            });
        })
        .detach();
    }

    fn prefetch_next(&mut self, cx: &mut Context<Self>) {
        let mut want = Vec::new();
        for lookahead in [1, 2] {
            let Some(&qix) = self.order.get(self.order_pos + lookahead) else {
                break;
            };
            let track = self.queue[qix].clone();
            if !self.downloads.contains_key(&track.id) {
                want.push(track);
            }
        }
        for track in want {
            // from_ms/autoplay не важны: результат ляжет в кеш
            self.request_download(track, usize::MAX, 0, false, cx);
        }
        self.prime_downloads();
    }

    fn prime_downloads(&mut self) {
        // держим в памяти только текущий и ближайшие треки
        if self.downloads.len() <= 6 {
            return;
        }
        let keep: HashSet<String> = self
            .current
            .iter()
            .map(|c| c.id.clone())
            .chain(
                self.order
                    .iter()
                    .skip(self.order_pos + 1)
                    .take(4)
                    .map(|&ix| self.queue[ix].id.clone()),
            )
            .collect();
        let stale: Vec<String> = self.downloads.keys().filter(|id| !keep.contains(*id)).cloned().collect();
        for id in stale {
            self.downloads.remove(&id);
        }
    }

    // --- восстановление ---

    pub fn restore(&mut self, saved: SavedSession, autoplay: bool, cx: &mut Context<Self>) {
        if saved.queue.is_empty() {
            return;
        }
        let current = saved
            .order
            .get(saved.order_pos)
            .and_then(|&q| saved.queue.get(q))
            .cloned();
        self.queue = saved.queue;
        self.order = if saved.order.is_empty() {
            (0..self.queue.len()).collect()
        } else {
            saved.order
        };
        self.order_pos = saved.order_pos.min(self.order.len() - 1);
        self.shuffle = saved.shuffle;
        self.repeat = match saved.repeat {
            1 => RepeatMode::All,
            2 => RepeatMode::One,
            _ => RepeatMode::Off,
        };
        self.context_name = "Прошлая очередь".into();
        if let Some(track) = current {
            self.begin_track_restored(&track, saved.position_ms as i64, autoplay, cx);
        }
    }

    fn begin_track_restored(&mut self, track: &TrackDto, from_ms: i64, autoplay: bool, cx: &mut Context<Self>) {
        // не запускаем звук сам: готовим трек, но ставим на паузу
        self.gen += 1;
        let gen = self.gen;
        self.current = Some(track.clone());
        self.started = false;
        self.playing = false;
        let autoplay_restored_flag = autoplay;
        if let Some(bytes) = self.downloads.get(&track.id).cloned() {
            self.apply_bytes(gen, &bytes, from_ms, autoplay, cx);
        } else {
            self.loading = true;
            let id = track.id.clone();
            let api = self.yandex();
            let quality = self.quality;
            let weak = cx.entity().downgrade();
            if self.inflight.insert(id.clone()) {
                cx.spawn(async move |_, cx| {
                    let res = async {
                        let stream = api.stream(&id, quality).await?;
                        api.download(&stream).await
                    }
                    .await;
                    let _ = weak.update(cx, |this, cx| {
                        this.inflight.remove(&id);
                        match res {
                            Ok(bytes) => {
                                this.downloads.insert(id.clone(), Arc::clone(&bytes));
                                if gen == this.gen {
                                    this.loading = false;
                                    this.start_from_bytes(&bytes, from_ms.max(0), autoplay_restored_flag, cx);
                                }
                            }
                            Err(e) => {
                                if gen == this.gen {
                                    this.loading = false;
                                    this.error = Some(e.to_string().into());
                                }
                            }
                        }
                    });
                })
                .detach();
            }
        }
        cx.emit(PlayerEvent::State);
    }

    pub fn save_session_now(&mut self, cx: &mut Context<Self>) {
        self.last_session_save = std::time::Instant::now();
        let saved = SavedSession {
            queue: self.queue.clone(),
            order: self.order.clone(),
            order_pos: self.order_pos,
            position_ms: self.position_ms().max(0) as u64,
            shuffle: self.shuffle,
            repeat: match self.repeat {
                RepeatMode::Off => 0,
                RepeatMode::All => 1,
                RepeatMode::One => 2,
            },
            wave_station: self.wave.as_ref().map(|w| w.station.clone()),
            saved_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        };
        self.store.update(cx, |store, _| {
            store.save_session(&saved);
        });
    }
}
