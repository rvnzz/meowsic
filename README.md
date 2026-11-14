# meowsic

Неофициальный десктоп-клиент Яндекс Музыки на **GPUI** (gpui-kit).
Клон по функционалу [mashiro](https://github.com/aeviww/yandex-music-client),
но без веб-стека: один Rust-бэкенд и нативный UI на элементах GPUI.

> «Яндекс» к проекту отношения не имеет. Используется неофициальное API,
> нужен аккаунт с активной подпиской Плюс.

## Что уже работает (ядро)

- **Авторизация** по OAuth-токену: вставка в настройках, проверка через
  `/account/status`, токен хранится локально и не покидает Rust-часть.
- **Главная**: приветствие, hero «Моя волна», персональное колесо (`/wheel/new`)
  с карточками волн, альбомов, артистов и плейлистов.
- **Моя волна** (`/rotor`): старт сессии, лайк/дизлайк/скип с feedback в Яндексовский
  rotor, подтягивание следующей пачки треков заранее, настройки станции
  (язык, настроение) через `settings3`.
- **Поиск**: треки / артисты / альбомы, запуск по Enter, переход на страницы.
- **Библиотека**: лайки и дизлайки (`/users/{uid}/likes|dislikes/tracks`),
  плейлисты — создание, переименование, удаление (`playlists/create`,
  `change-relative`), добавление и удаление треков, страница «Мне нравится».
- **Страницы альбома, артиста, плейлиста** с запуском всего списка.
- **Плеер** (rodio + symphonia): очередь, перемешивание (Fisher–Yates с сохранением
  текущего трека), три режима повтора (выкл / очередь / трек), пауза/перемотка
  (`try_seek`), громкость, авто-предзагрузка следующих треков, кеш скачанных тел.
- **Восстановление сессии**: очередь, позиция и режимы переживают перезапуск
  (`session.json`), отдельные тумблеры «восстанавливать» и «автозапуск».
- **Тексты песен** из [LRCLIB](https://lrclib.net): синхронная подсветка строки,
  автопрокрутка (окно строк), переход по клику, панель справа (`Ctrl-L`).
- **Эквалайзер**: 8 полос (RBJ peaking биквады прямо в потоке сэмплов, свой
  `rodio::Source`), пресеты, шум на лету без перезапуска трека.
- **Таймер сна** с остановкой воспроизведения.
- **Горячие клавиши**: `Space` — play/pause (кроме полей ввода), `Ctrl-→/←` —
  скип, `Ctrl-L` — тексты, `Ctrl-Q` — очередь, `F11` — поиск.
- Тёмная/светлая тема, свой акцентный цвет, обложки по URL через GPUI-картинки.

## Сборка

Только Rust-стабильный и системные зависимости GPUI (на Arch — из коробки для
Wayland/X11) плюс ALSA для rodio:

```sh
cargo build --release
./target/release/meowsic
```

Тесты: `cargo test` (DSP эквалайзера + сетевые канарейки API/LRCLIB).

## Релизы и пакеты

CI (`.github/workflows/release.yml`) запускается **только на тегах `v*`** и
складывает артефакты в GitHub Release:

| платформа | артефакт | как собрано |
| --- | --- | --- |
| Arch Linux | `meowsic-X.Y.Z-x86_64.pkg.tar.zst` | `makepkg` в контейнере archlinux, PKGBUILD в `packaging/arch` |
| любой Linux | `meowsic-X.Y.Z-x86_64.AppImage` | ubuntu-22.04 + linuxdeploy, desktop/иконка в `packaging/linux` |
| Windows x64 | `meowsic-X.Y.Z-win64-setup.exe` | NSIS-установщик, скрипт в `packaging/windows` |

Новый релиз:

```sh
cargo pkg set version --version X.Y.Z   # или руками Cargo.toml
git commit -am "release X.Y.Z" && git tag -a vX.Y.Z -m "..."
git push origin main --follow-tags      # аннотированный тег — триггер CI
```

## Хранение данных

`~/.config/meowsic/`:

| файл | что хранит |
| --- | --- |
| `settings.json` | токен, тема, акцент, качество, EQ, тумблеры плеера |
| `session.json` | очередь, позиция, перемешивание, повтор, станция волны |

## Архитектура

```
src/
├── main.rs            # bootstrap: init, тема, HttpClient, окно
├── net.rs             # tokio-рантайм для reqwest (GPUI — не tokio)
├── http.rs            # gpui HttpClient → img() грузит обложки
├── store.rs           # Settings / session persistence
├── session.rs         # токен + профиль, события логина
├── lib_index.rs       # кеш лайков/дизлайков/плейлистов
├── yandex/            # API-клиент: account/catalog/library/wave/stream/lyrics
│                      #   download-info + XML + md5(get-mp3) как в mashiro
├── player/            # Entity<Player>: очередь, rodio-движок, prefetch, sleep
│   └── eq.rs          # 8-полосный biquad Source
└── ui/                # оболочка MusicApp: сайдбар, страницы, плеер-бар, тексты
    ├── home.rs wave.rs search.rs library.rs list_page.rs settings.rs
    └── widgets.rs     # TrackRow, Cover, SettingRow, баннеры
```

Состояние живёт в entity-моделях (`Session`, `Player`, `LibIndex`), страницы —
отдельные Render-вью, получающие общие ручки через `PageCtx`. Сетевой код никогда
не поллится на GPUI-исполнителе: каждый запрос стартует на отдельном tokio-рантайме,
в UI приходит `JoinHandle`-аваит и проверка поколения/idd, чтобы устаревшие
ответы не портили состояние.

## Известные ограничения

- API неофициальное и может измениться в любой момент.
- Без подписки Плюс воспроизведение недоступно.
- Лоссless-путь (get-file-info + HMAC) пока не подключён: играем mp3 до 320.

## Дорога до полного клона mashiro

Порядок следующих фаз:

1. обход цензуры (fckcensordata + свои замены), метка AI-музыки (slopless);
2. загрузки в папку и воспроизведение локальных файлов;
3. Discord rich presence, трей, глобальные хоткеи;
4. кроссфейд, скорость воспроизведения, визуализатор;
5. «слушать вместе» (relay + локалка), мини-плеер, редактор макета;
6. статистика прослушиваний, genius, экспорт/импорт плейлистов.

## Благодарности

- [aeviww/yandex-music-client (mashiro)](https://github.com/aeviww/yandex-music-client) —
  референс функционала и схемы API;
- [MarshalX/yandex-music-api](https://github.com/MarshalX/yandex-music-api) — описание API;
- [lrclib](https://lrclib.net) — синхронные тексты.

Лицензия: GPL-3.0 (как у оригинального mashiro).
