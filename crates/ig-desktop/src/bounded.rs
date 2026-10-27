//! Guards for webview commands that hit git or the disk: at most `LIMIT` run at once (the rest
//! wait as futures, not threads).

use std::sync::Arc;
use tokio::sync::Semaphore;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use std::time::Duration;

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
