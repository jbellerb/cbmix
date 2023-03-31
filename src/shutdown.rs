use tokio::sync::{mpsc, watch};

#[derive(Debug)]
pub struct Sender {
    notify: watch::Sender<()>,
    complete_tx: mpsc::Sender<()>,
    complete_rx: mpsc::Receiver<()>,
}

impl Sender {
    pub fn new() -> Self {
        let (notify, _) = watch::channel(());
        let (complete_tx, complete_rx) = mpsc::channel(1);

        Self {
            notify,
            complete_tx,
            complete_rx,
        }
    }

    pub fn subscribe(&self) -> Receiver {
        let notify = self.notify.subscribe();
        let complete = self.complete_tx.clone();

        Receiver { notify, complete }
    }

    pub async fn recv(&mut self) {
        let _ = self.complete_rx.recv().await;
    }

    pub async fn shutdown(mut self) {
        let _ = self.notify.send(());

        drop(self.complete_tx);
        let _ = self.complete_rx.recv().await;
    }
}

impl Default for Sender {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct Receiver {
    notify: watch::Receiver<()>,
    complete: mpsc::Sender<()>,
}

impl Receiver {
    pub async fn recv(&mut self) {
        let _ = self.notify.changed().await;
    }

    pub async fn force_shutdown(self) {
        let _ = self.complete.send(()).await;
    }
}
