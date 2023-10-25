use std::time::Duration;
use tokio::{select};
use tokio::time::sleep;
use tokio::sync::mpsc::{Receiver, channel};
use tokio::sync::mpsc::error::TryRecvError;

pub struct GroupedWithin<I>
where I: 'static + Send {
    outlet: Receiver<Vec<I>>
}

impl<I> GroupedWithin<I>
where I: 'static + Send {
    pub fn new(group_size: usize,
               window_time: Duration,
               mut inlet: Receiver<Vec<I>>,
               buffer: usize) -> Self {
        let (tx, outlet) = channel::<Vec<I>>(buffer);
        tokio::spawn(async move {
            let mut window = Vec::with_capacity(group_size);

            loop {
                let grouped_fut = async {
                    while let Some(c) = inlet.recv().await {
                        window.extend(c);
                        if window.len() > group_size {
                            let will_send: Vec<I> = window.drain(0..group_size).collect();
                            return Some(will_send)
                        }
                    }
                    return None
                };

                let grouped = select! {
                    _ = sleep(window_time) => {
                        window.drain(..).collect()
                    },
                    grouped_opt = grouped_fut => {
                        match grouped_opt {
                            None => break,
                            Some(grouped) => grouped
                        }
                    }
                };

                if let Err(e) = tx.send(grouped).await {
                    tracing::error!("{}", e);
                }
            }
        });
        Self {
            outlet
        }
    }

    pub fn next(&mut self) -> Result<Vec<I>, TryRecvError> {
        self.outlet.try_recv()
    }
}