use pontia_application::{
    AppState,
    client_contract::{ClientOperation, ClientService, ClientServicePhase, test_registration},
    clients::ClientRegistry,
};
use pontia_core::{Error, Result};
use std::{
    future::{Future, poll_fn},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::Poll,
};
use tokio::task::JoinHandle;

struct Service {
    phase: ClientServicePhase,
    starts: Arc<AtomicUsize>,
    stopped: Arc<AtomicBool>,
    fail_start: bool,
    fail_task: bool,
}

impl ClientService for Service {
    fn phase(&self) -> ClientServicePhase {
        self.phase
    }

    fn start<'a>(&'a self, state: AppState) -> ClientOperation<'a, Option<JoinHandle<Result<()>>>> {
        Box::pin(async move {
            self.starts.fetch_add(1, Ordering::SeqCst);
            if self.fail_start {
                return Err(Error::StateConflict("service unavailable".into()));
            }
            let mut shutdown = state.shutdown().subscribe();
            let stopped = self.stopped.clone();
            let fail_task = self.fail_task;
            Ok(Some(tokio::spawn(async move {
                shutdown.wait_for(|stop| *stop).await.unwrap();
                stopped.store(true, Ordering::SeqCst);
                if fail_task {
                    Err(Error::StateConflict("service failed".into()))
                } else {
                    Ok(())
                }
            })))
        })
    }
}

struct TaskService(Mutex<Option<JoinHandle<Result<()>>>>);

impl ClientService for TaskService {
    fn phase(&self) -> ClientServicePhase {
        ClientServicePhase::Transport
    }

    fn start<'a>(
        &'a self,
        _state: AppState,
    ) -> ClientOperation<'a, Option<JoinHandle<Result<()>>>> {
        Box::pin(async move { Ok(self.0.lock().unwrap().take()) })
    }
}

struct Resource(Arc<AtomicBool>);

impl Drop for Resource {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn register_service(
    clients: &mut ClientRegistry,
    name: &'static str,
    service: Arc<dyn ClientService>,
) {
    let mut client = test_registration();
    let mut spec = client.spec.clone();
    spec.client_type = name;
    client.spec = Box::leak(Box::new(spec));
    client.service = Some(service);
    clients.register(client);
}

fn registry(service: Service) -> ClientRegistry {
    let mut client = test_registration();
    client.service = Some(Arc::new(service));
    let mut clients = ClientRegistry::default();
    clients.register(client);
    clients
}

async fn state(root: &std::path::Path, clients: ClientRegistry) -> AppState {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    AppState::builder(pool, root.to_path_buf())
        .clients(clients)
        .build()
}

#[tokio::test]
async fn starts_the_requested_phase_and_waits_for_service_shutdown() {
    let root = tempfile::tempdir().unwrap();
    let starts = Arc::new(AtomicUsize::new(0));
    let stopped = Arc::new(AtomicBool::new(false));
    let clients = registry(Service {
        phase: ClientServicePhase::Transport,
        starts: starts.clone(),
        stopped: stopped.clone(),
        fail_start: false,
        fail_task: false,
    });
    let state = state(root.path(), clients.clone()).await;

    clients
        .start_services(state.clone(), ClientServicePhase::Observation)
        .await
        .unwrap()
        .join()
        .await
        .unwrap();
    assert_eq!(starts.load(Ordering::SeqCst), 0);

    let tasks = clients
        .start_services(state.clone(), ClientServicePhase::Transport)
        .await
        .unwrap();
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert!(!stopped.load(Ordering::SeqCst));
    state.shutdown().notify();
    tasks.join().await.unwrap();
    assert!(stopped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn propagates_startup_failure() {
    let root = tempfile::tempdir().unwrap();
    let clients = registry(Service {
        phase: ClientServicePhase::Transport,
        starts: Arc::new(AtomicUsize::new(0)),
        stopped: Arc::new(AtomicBool::new(false)),
        fail_start: true,
        fail_task: false,
    });
    let state = state(root.path(), clients.clone()).await;

    assert!(matches!(
        clients
            .start_services(state, ClientServicePhase::Transport)
            .await,
        Err(Error::StateConflict(_))
    ));
}

#[tokio::test]
async fn propagates_background_failure_when_joined() {
    let root = tempfile::tempdir().unwrap();
    let clients = registry(Service {
        phase: ClientServicePhase::Transport,
        starts: Arc::new(AtomicUsize::new(0)),
        stopped: Arc::new(AtomicBool::new(false)),
        fail_start: false,
        fail_task: true,
    });
    let state = state(root.path(), clients.clone()).await;
    let tasks = clients
        .start_services(state.clone(), ClientServicePhase::Transport)
        .await
        .unwrap();

    state.shutdown().notify();
    assert!(matches!(tasks.join().await, Err(Error::StateConflict(_))));
}

#[tokio::test]
async fn replacing_a_registration_does_not_start_the_previous_service() {
    let root = tempfile::tempdir().unwrap();
    let previous_starts = Arc::new(AtomicUsize::new(0));
    let starts = Arc::new(AtomicUsize::new(0));
    let mut clients = registry(Service {
        phase: ClientServicePhase::Transport,
        starts: previous_starts.clone(),
        stopped: Arc::new(AtomicBool::new(false)),
        fail_start: false,
        fail_task: false,
    });
    let mut replacement = test_registration();
    replacement.service = Some(Arc::new(Service {
        phase: ClientServicePhase::Transport,
        starts: starts.clone(),
        stopped: Arc::new(AtomicBool::new(false)),
        fail_start: false,
        fail_task: false,
    }));
    clients.register(replacement);
    let state = state(root.path(), clients.clone()).await;

    let tasks = clients
        .start_services(state.clone(), ClientServicePhase::Transport)
        .await
        .unwrap();
    assert_eq!(previous_starts.load(Ordering::SeqCst), 0);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    state.shutdown().notify();
    tasks.join().await.unwrap();
}

#[tokio::test]
async fn startup_failure_releases_resources_of_previously_started_services() {
    let root = tempfile::tempdir().unwrap();
    let released = Arc::new(AtomicBool::new(false));
    let resource = Resource(released.clone());
    let task = tokio::spawn(async move {
        let _resource = resource;
        std::future::pending::<()>().await;
        Ok(())
    });
    let mut clients = ClientRegistry::default();
    register_service(
        &mut clients,
        "first-service",
        Arc::new(TaskService(Mutex::new(Some(task)))),
    );
    register_service(
        &mut clients,
        "failing-service",
        Arc::new(Service {
            phase: ClientServicePhase::Transport,
            starts: Arc::new(AtomicUsize::new(0)),
            stopped: Arc::new(AtomicBool::new(false)),
            fail_start: true,
            fail_task: false,
        }),
    );
    let state = state(root.path(), clients.clone()).await;

    assert!(matches!(
        clients
            .start_services(state, ClientServicePhase::Transport)
            .await,
        Err(Error::StateConflict(_))
    ));
    assert!(released.load(Ordering::SeqCst));
}

#[tokio::test]
async fn drains_remaining_services_after_a_task_error_or_panic() {
    for panics in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let failed = tokio::spawn(async move {
            assert!(!panics, "simulated service panic");
            Err(Error::StateConflict("service failed".into()))
        });
        while !failed.is_finished() {
            tokio::task::yield_now().await;
        }
        let release = Arc::new(tokio::sync::Notify::new());
        let released = Arc::new(AtomicBool::new(false));
        let resource = Resource(released.clone());
        let task_release = release.clone();
        let remaining = tokio::spawn(async move {
            let _resource = resource;
            task_release.notified().await;
            Ok(())
        });
        let mut clients = ClientRegistry::default();
        register_service(
            &mut clients,
            "failing-service",
            Arc::new(TaskService(Mutex::new(Some(failed)))),
        );
        register_service(
            &mut clients,
            "remaining-service",
            Arc::new(TaskService(Mutex::new(Some(remaining)))),
        );
        let state = state(root.path(), clients.clone()).await;
        let tasks = clients
            .start_services(state, ClientServicePhase::Transport)
            .await
            .unwrap();

        let mut join = std::pin::pin!(tasks.join());
        assert!(poll_fn(|cx| Poll::Ready(join.as_mut().poll(cx).is_pending())).await);
        assert!(!released.load(Ordering::SeqCst));
        release.notify_one();
        let result = join.await;
        if panics {
            assert!(matches!(result, Err(Error::Domain(_))));
        } else {
            assert!(matches!(result, Err(Error::StateConflict(_))));
        }
        assert!(released.load(Ordering::SeqCst));
    }
}
