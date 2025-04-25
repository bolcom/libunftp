use std::fmt::Debug;
use tokio::sync::{Mutex, RwLock};
use tokio::sync::{broadcast, mpsc};

// Notifier lets other tasks know that we're shutting down.
#[derive(Debug)]
pub struct Notifier {
    shutdown_tx: RwLock<Option<broadcast::Sender<()>>>,
    shutdown_complete_tx: RwLock<Option<mpsc::Sender<()>>>,
    shutdown_complete_rx: Mutex<mpsc::Receiver<()>>,
}

impl Notifier {
    // Creates a new Shutdown notifier
    pub fn new() -> Notifier {
        let (shutdown_tx, _) = broadcast::channel(1);
        let (shutdown_complete_tx, shutdown_complete_rx) = mpsc::channel(1);
        Notifier {
            shutdown_tx: RwLock::new(Some(shutdown_tx)),
            shutdown_complete_tx: RwLock::new(Some(shutdown_complete_tx)),
            shutdown_complete_rx: Mutex::new(shutdown_complete_rx),
        }
    }

    // Notifies shutdown listeners that shutdown is commencing. Listeners then need to gracefully
    // shutdown and signal that they are done by simply letting the Listener instance that they hold
    // go out of scope.
    pub async fn notify(&self) {
        // When the sender is dropped, all tasks which have `subscribe`d will
        // receive the shutdown signal and can exit
        drop(self.shutdown_tx.write().await.take());
        // Drop final `Sender` so the `Receiver` used in linger() will complete
        drop(self.shutdown_complete_tx.write().await.take())
    }

    // Waits for tasks holding shutdown listeners to finish
    pub async fn linger(&self) {
        // Wait for all active connections to finish processing. As the `Sender`
        // handle held by the listener has been dropped above, the only remaining
        // `Sender` instances are held by connection handler tasks. When those drop,
        // the `mpsc` channel will close and `recv()` will return `None`.
        let _ = self.shutdown_complete_rx.lock().await.recv().await;
    }

    pub async fn subscribe(&self) -> Listener {
        let sender_opt = self.shutdown_tx.read().await;
        let complete_sender_opt = self.shutdown_complete_tx.read().await;
        Listener {
            shutdown: sender_opt.is_none(),
            shutdown_rx: sender_opt.as_ref().map(|tx| tx.subscribe()),
            shutdown_complete_tx: complete_sender_opt.clone(),
        }
    }
}

// Listener listens for shutdown notifications
#[derive(Debug)]
#[allow(dead_code)]
pub struct Listener {
    shutdown: bool,
    shutdown_rx: Option<broadcast::Receiver<()>>,
    shutdown_complete_tx: Option<mpsc::Sender<()>>,
}

impl Listener {
    /// Returns `true` if the shutdown signal has been received.
    pub(crate) fn is_shutdown(&self) -> bool {
        self.shutdown
    }

    /// Receive the shutdown notice, waiting if necessary.
    pub async fn listen(&mut self) {
        // If the shutdown signal has already been received, then return
        // immediately.
        if self.is_shutdown() {
            return;
        }

        // Cannot receive a "lag error" as only one value is ever sent.
        let _ = self.shutdown_rx.as_mut().unwrap().recv().await;

        // Remember that the signal has been received.
        self.shutdown = true;
    }
}
