//! Quitting (Cmd+Q, the app menu) waits for the board's last edit: the exit is held, the webview
//! gets `flush-before-exit`, and the app exits on its `exit_flushed` ack or after a timeout.

use std::sync::{Condvar, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// How long a quit waits for the webview's flush.
pub const FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

/// (exit held once, ack came)
#[derive(Default)]
pub struct ExitGate {
    s: Mutex<(bool, bool)>,
    cv: Condvar,
}

impl ExitGate {
    /// The first exit request holds the exit (true); later ones, our own exit among them, pass.
    pub fn begin(&self) -> bool {
        let mut g = self.s.lock().unwrap();
        !std::mem::replace(&mut g.0, true)
    }

    pub fn ack(&self) {
        self.s.lock().unwrap().1 = true;
        self.cv.notify_all();
    }

    /// Waits at most `t` for the ack; whether it came.
    pub fn wait(&self, t: Duration) -> bool {
        let g = self.s.lock().unwrap();
        let (g, _) = self.cv.wait_timeout_while(g, t, |s| !s.1).unwrap();
        g.1
    }
}

/// The webview wrote what it had: the held exit goes on.
#[tauri::command]
pub fn exit_flushed(gate: tauri::State<'_, ExitGate>) {
    gate.ack();
}

/// `RunEvent::ExitRequested`: hold it while a window can still flush, then exit.
pub fn requested(app: &AppHandle, api: &tauri::ExitRequestApi, code: Option<i32>) {
    if app.webview_windows().is_empty() || !app.state::<ExitGate>().begin() {
        return;
    }
    api.prevent_exit();
    let _ = app.emit("flush-before-exit", ());
    let app = app.clone();
    std::thread::spawn(move || {
        if !app.state::<ExitGate>().wait(FLUSH_TIMEOUT) {
            eprintln!("ig: the board did not confirm its last save in time; quitting");
        }
        app.exit(code.unwrap_or(0));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Instant;

    #[test]
    fn only_the_first_request_holds_the_exit() {
        let g = ExitGate::default();
        assert!(g.begin());
        assert!(!g.begin());
    }

    #[test]
    fn an_ack_releases_the_wait_at_once() {
        let g = Arc::new(ExitGate::default());
        let g2 = g.clone();
        let t = Instant::now();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            g2.ack();
        });
        assert!(g.wait(Duration::from_secs(5)));
        assert!(t.elapsed() < Duration::from_secs(2));
        // an ack that came before the wait counts too
        let early = ExitGate::default();
        early.ack();
        assert!(early.wait(Duration::from_millis(1)));
    }

    #[test]
    fn no_ack_times_out() {
        let g = ExitGate::default();
        let t = Instant::now();
        assert!(!g.wait(Duration::from_millis(50)));
        assert!(t.elapsed() >= Duration::from_millis(50));
    }
}
