use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tokio::sync::watch;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct OnlineDevices(Arc<Mutex<HashMap<Uuid, Connection>>>);

struct Connection {
    id: Uuid,
    cancel: watch::Sender<bool>,
}

pub(crate) struct ConnectionLease {
    devices: OnlineDevices,
    device_id: Uuid,
    connection_id: Uuid,
}

impl OnlineDevices {
    pub fn connection_id(&self, device_id: Uuid) -> Option<Uuid> {
        self.0
            .lock()
            .expect("online devices lock")
            .get(&device_id)
            .map(|entry| entry.id)
    }

    pub(crate) fn register(&self, device_id: Uuid) -> (ConnectionLease, watch::Receiver<bool>) {
        let id = Uuid::new_v4();
        let (cancel, receiver) = watch::channel(false);
        let mut devices = self.0.lock().expect("online devices lock");
        if let Some(old) = devices.insert(device_id, Connection { id, cancel }) {
            old.cancel.send_replace(true);
        }
        (
            ConnectionLease {
                devices: self.clone(),
                device_id,
                connection_id: id,
            },
            receiver,
        )
    }
}

impl Drop for ConnectionLease {
    fn drop(&mut self) {
        let mut devices = self.devices.0.lock().expect("online devices lock");
        if devices
            .get(&self.device_id)
            .is_some_and(|entry| entry.id == self.connection_id)
        {
            devices.remove(&self.device_id);
        }
    }
}
