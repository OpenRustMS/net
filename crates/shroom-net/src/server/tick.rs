use std::time::Duration;

use tokio::{sync::watch, task::JoinHandle, time::Instant};

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct TickUnit(u64);

pub struct Tick(watch::Receiver<TickUnit>);

impl Tick {
    pub async fn next(&mut self) -> TickUnit {
        self.0.changed().await.expect("Tick");
        *self.0.borrow()
    }
}

pub struct Ticker {
    gen: JoinHandle<()>,
    rx: watch::Receiver<TickUnit>,
}

impl Drop for Ticker {
    fn drop(&mut self) {
        self.gen.abort();
    }
}

impl Ticker {
    pub fn spawn(tick_dur: Duration) -> Self {
        let (tx, rx) = watch::channel(TickUnit(0));
        let gen = tokio::spawn(async move {
            let mut interval = tokio::time::interval_at(Instant::now() + tick_dur, tick_dur);
            let mut ticks = 0;
            loop {
                interval.tick().await;
                tx.send(TickUnit(ticks)).expect("Ticks");
                ticks += 1;
            }
        });
        Self { gen, rx }
    }

    pub fn spawn_from_millis(millis: u64) -> Self {
        Self::spawn(Duration::from_millis(millis))
    }

    pub fn get_tick(&self) -> Tick {
        Tick(self.rx.clone())
    }
}
