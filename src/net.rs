//!Tokio-рантайм для reqwest: GPUI сам по себе не tokio-исполнитель,
//!поэтому весь сетевой обмен живёт на отдельном пуле потоков.

use std::sync::LazyLock;

pub(crate) static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("не удалось запустить сетевой рантайм")
});

/// Запускает future на сетевом рантайме и ждёт результат из GPUI.
/// [`tokio::task::JoinHandle`] можно поллить в любом исполнителе.
pub(crate) async fn spawn_net<T, F>(future: F) -> Result<T, String>
where
    T: Send + 'static,
    F: std::future::Future<Output = T> + Send + 'static,
{
    RUNTIME
        .spawn(future)
        .await
        .map_err(|e| format!("сеть: задача прервана: {e}"))
}
