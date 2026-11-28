use pontia_core::{Error, Result};
use tokio::task::JoinSet;

#[derive(Default)]
pub(super) struct BackgroundTasks {
    tasks: JoinSet<Result<()>>,
}

impl BackgroundTasks {
    pub(super) fn spawn(&mut self, task: impl Future<Output = ()> + Send + 'static) {
        self.spawn_service(async move {
            task.await;
            Ok(())
        });
    }

    pub(super) fn spawn_service(
        &mut self,
        task: impl Future<Output = Result<()>> + Send + 'static,
    ) {
        self.tasks.spawn(task);
    }

    pub(super) async fn join(mut self) -> Result<()> {
        let mut outcome = Ok(());
        while let Some(result) = self.tasks.join_next().await {
            let result = result
                .map_err(|error| Error::Domain(error.to_string()))
                .and_then(|result| result);
            outcome = outcome.and(result);
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn a_failed_service_still_waits_for_the_last_subscription() {
        let mut background = BackgroundTasks::default();
        let (failed, failure) = oneshot::channel();
        let (finish, finishing) = oneshot::channel();
        let (done, completed) = oneshot::channel();
        background.spawn_service(async {
            failed.send(()).unwrap();
            Err(Error::Domain("service failed".into()))
        });
        background.spawn(async {
            finishing.await.unwrap();
            done.send(()).unwrap();
        });
        let joined = tokio::spawn(background.join());
        failure.await.unwrap();
        finish.send(()).unwrap();
        assert!(joined.await.unwrap().is_err());
        completed.await.unwrap();
    }
}
