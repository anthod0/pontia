use super::*;
use tokio::sync::watch;

/// A connection-local cursor. Dropping the tail releases the commit watch.
pub struct EventTail {
    store: EventSubscriptionRepository,
    scope: EventScope,
    after: i64,
    head: Option<i64>,
    commits: watch::Receiver<i64>,
    once: bool,
    buffered: std::vec::IntoIter<PositionedEvent>,
}

impl EventTail {
    pub(super) fn new(
        service: &EventDeliveryService,
        scope: EventScope,
        after: i64,
        once: bool,
    ) -> Self {
        Self {
            store: service.store.clone(),
            scope,
            after: after.max(0),
            once,
            head: None,
            commits: service.broker.head_position.subscribe(),
            buffered: Default::default(),
        }
    }

    pub async fn next(&mut self) -> Result<Option<PositionedEvent>> {
        loop {
            if let Some(event) = self.buffered.next() {
                return Ok(Some(event));
            }
            let head = match self.head {
                Some(head) => head,
                None => {
                    let head = self.store.current_head_position().await?;
                    self.head = Some(head);
                    head
                }
            };
            if self.after < head {
                let page = self
                    .store
                    .scan(&self.scope, &[], self.after, head, WINDOW)
                    .await?;
                self.after = page.end_position;
                self.buffered = page.events.into_iter();
            } else if self.once || self.commits.changed().await.is_err() {
                return Ok(None);
            } else {
                self.head = Some(head.max(*self.commits.borrow_and_update()));
            }
        }
    }
}
