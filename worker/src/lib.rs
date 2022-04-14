use chrono::TimeZone;
pub use clokwerk::{AsyncScheduler, Interval, Job, TimeUnits};
use tokio::select;
pub use tokio_util::sync::CancellationToken;

pub struct Worker<Source, Tz>
where
    Source: Fn(&mut AsyncScheduler<Tz>) -> eyre::Result<()>,
    Tz: TimeZone + Send + Sync + 'static,
    <Tz as TimeZone>::Offset: Send
{
    source: Source,
    cancel_token_root: CancellationToken,
    timezone: Tz,
    current_token: Option<CancellationToken>,
}

impl<Source, Tz> Worker<Source, Tz>
where
    Source: Fn(&mut AsyncScheduler<Tz>) -> eyre::Result<()>,
    Tz: TimeZone + Send + Sync + 'static,
    <Tz as TimeZone>::Offset: Send
{
    /// Create a new worker with given source closure and root cancellation token.
    pub fn new(source: Source, cancel_token_root: CancellationToken, tz: Tz) -> Self {
        Self {
            source,
            cancel_token_root,
            timezone: tz,
            current_token: None,
        }
    }

    /// Cancel any previous scheduler and start a new one.
    pub fn try_schedule(&mut self) -> eyre::Result<()> {
        self.stop();

        let mut scheduler = AsyncScheduler::with_tz(self.timezone.clone());
        (self.source)(&mut scheduler)?;

        let token = self.cancel_token_root.child_token();
        self.current_token = Some(token.clone());

        tokio::spawn(async move {
            
            loop {
                // Runs scheduler every second as long as it hasn't been cancelled
                select! {
                    _ = token.cancelled() => { return }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                        scheduler.run_pending().await;
                    }
                }
            }
        });
        Ok(())
    }

    /// Cancel any current scheduler.
    pub fn stop(&mut self) {
        if let Some(token) = &self.current_token {
            token.cancel()
        }
    }
}
