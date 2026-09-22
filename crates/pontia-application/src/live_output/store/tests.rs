use crate::live_output::{LiveOutputIdentity, LiveOutputProducer};

mod lifecycle;
mod snapshots;
mod subscriptions;

fn identity() -> LiveOutputIdentity {
    LiveOutputIdentity {
        session_id: "sess_1".into(),
        turn_id: "turn_1".into(),
        stream_id: "stream_1".into(),
    }
}

fn producer() -> LiveOutputProducer {
    LiveOutputProducer {
        identity: identity(),
        runtime_instance_id: "rtinst_1".into(),
    }
}
