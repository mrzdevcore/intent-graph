//! Guards for webview commands that hit git or the disk: at most `LIMIT` run at once (the rest
//! wait as futures, not threads), and the project's file list is computed once per generation.

use std::sync::{Arc, Mutex};
use tokio::sync::{Mutex as AsyncMutex, Semaphore};

const LIMIT: usize = 4;

pub struct Gate(Arc<Semaphore>);

impl Default for Gate {
    fn default() -> Self {
        Gate(Arc::new(Semaphore::new(LIMIT)))
    }
}

impl Gate {
    /// Runs `f` on a blocking thread once one of the `LIMIT` slots is free.
    pub async fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, String> {
        let _slot = self.0.acquire().await.map_err(|e| e.to_string())?;
        tauri::async_runtime::spawn_blocking(f)
            .await
            .map_err(|e| e.to_string())
    }
}

/// The project's file list. The number counts clears: a load that started before one never
/// fills the cache. Concurrent cold calls share one load.
#[derive(Default)]
pub struct FileList {
    cached: Mutex<(u64, Option<Arc<Vec<String>>>)>,
    flight: AsyncMutex<()>,
}

impl FileList {
    pub fn clear(&self) {
        let mut g = self.cached.lock().unwrap();
        *g = (g.0 + 1, None);
    }

    pub async fn get(
        &self,
        load: impl FnOnce() -> Vec<String> + Send + 'static,
    ) -> Result<Arc<Vec<String>>, String> {
        if let Some(l) = self.cached.lock().unwrap().1.clone() {
            return Ok(l);
        }
        let _one = self.flight.lock().await;
        let gen_ = {
            let g = self.cached.lock().unwrap();
            if let Some(l) = g.1.clone() {
                return Ok(l);
            }
            g.0
        };
        let l = Arc::new(
            tauri::async_runtime::spawn_blocking(load)
                .await
                .map_err(|e| e.to_string())?,
        );
        let mut g = self.cached.lock().unwrap();
        if g.0 == gen_ {
            g.1 = Some(l.clone());
        }
        Ok(l)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use std::time::Duration;

    #[test]
    fn a_hundred_cold_calls_list_once_and_a_clear_during_a_load_is_not_cached() {
        tauri::async_runtime::block_on(async {
            let list = Arc::new(FileList::default());
            let runs = Arc::new(AtomicUsize::new(0));
            let hs: Vec<_> = (0..100)
                .map(|_| {
                    let (l, r) = (list.clone(), runs.clone());
                    tauri::async_runtime::spawn(async move {
                        l.get(move || {
                            r.fetch_add(1, SeqCst);
                            std::thread::sleep(Duration::from_millis(100));
                            vec!["a".into()]
                        })
                        .await
                    })
                })
                .collect();
            for h in hs {
                assert_eq!(h.await.unwrap().unwrap().len(), 1);
            }
            assert_eq!(runs.load(SeqCst), 1);

            list.clear();
            let l2 = list.clone();
            let h = tauri::async_runtime::spawn(async move {
                l2.get(|| {
                    std::thread::sleep(Duration::from_millis(200));
                    vec!["stale".into()]
                })
                .await
            });
            tokio::time::sleep(Duration::from_millis(50)).await;
            list.clear();
            assert_eq!(h.await.unwrap().unwrap()[0], "stale");
            let fresh = list.get(|| vec!["fresh".into()]).await.unwrap();
            assert_eq!(fresh[0], "fresh");
        });
    }

    #[test]
    fn the_gate_runs_at_most_four_at_once() {
        tauri::async_runtime::block_on(async {
            let gate = Arc::new(Gate::default());
            let (now, peak) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
            let hs: Vec<_> = (0..20)
                .map(|_| {
                    let (g, n, p) = (gate.clone(), now.clone(), peak.clone());
                    tauri::async_runtime::spawn(async move {
                        g.run(move || {
                            p.fetch_max(n.fetch_add(1, SeqCst) + 1, SeqCst);
                            std::thread::sleep(Duration::from_millis(30));
                            n.fetch_sub(1, SeqCst);
                        })
                        .await
                    })
                })
                .collect();
            for h in hs {
                h.await.unwrap().unwrap();
            }
            assert_eq!(peak.load(SeqCst), LIMIT);
        });
    }
}
