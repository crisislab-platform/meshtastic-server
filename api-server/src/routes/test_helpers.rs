use std::sync::{atomic::AtomicBool, Arc};

use axum_test::TestServer;
use bytes::{Bytes, BytesMut};
use prost::Message;
use tokio::sync::{broadcast, mpsc, Mutex};

use crate::{
    init_app,
    proto::meshtastic::CrisislabMessage,
    utils::BoundedVecDeque,
    AppSettings, AppState, MeshInterface,
};

pub struct MockMesh {
    mesh_interface: MeshInterface,
    outgoing_msg_receiver: mpsc::Receiver<Bytes>,
}

impl MockMesh {
    fn new() -> Self {
        let (sender_to_publisher, outgoing_msg_receiver) = mpsc::channel::<Bytes>(10);
        let (sender_to_subscribers, _) = broadcast::channel::<Bytes>(10);

        Self {
            mesh_interface: MeshInterface {
                sender_to_publisher,
                sender_to_subscribers,
            },
            outgoing_msg_receiver,
        }
    }

    /// Waits for a single CrisislabMessage to be received from an endpoint handler for the
    /// mesh. Panics if the channels was closed or the message failed to decode.
    pub async fn await_msg_for_mesh(&mut self) -> CrisislabMessage {
        CrisislabMessage::decode(
            self.outgoing_msg_receiver
                .recv()
                .await
                .expect("Channel for outgoing messages was closed"),
        )
        .expect("Failed to decode message for mesh")
    }

    /// Asserts that the channel is still alive, but no message has been received.
    pub fn assert_no_msg(&mut self) {
        match self.outgoing_msg_receiver.try_recv() {
            Err(mpsc::error::TryRecvError::Empty) => { /* good, no message */ }
            Err(mpsc::error::TryRecvError::Disconnected) => {
                panic!("Channel for outgoing messages was closed");
            }
            Ok(_) => panic!(
                "A message was received on the channel for outgoing messages but none was expected"
            ),
        }
    }

    /// Encodes and sends a CrisislabMessage on the broadcast channel as if it came from the
    /// mesh. Panics if failed to encode message, or failed to send on channel.
    pub async fn send_from_mesh(&self, message: CrisislabMessage) {
        let mut buffer = BytesMut::with_capacity(message.encoded_len());

        if let Err(error) = message.encode(&mut buffer) {
            panic!(
                "Failed to encode crisislab message as protobuf: {:?}",
                error
            );
        }

        if let Err(error) = self
            .mesh_interface
            .get_sender_to_subscriber()
            .send(buffer.freeze())
        {
            panic!(
                "Failed to send encoded crisislab message to endpoint handler: {:?}",
                error
            );
        }
    }

    fn clone_interface(&self) -> MeshInterface {
        self.mesh_interface.clone()
    }
}

fn create_app_state(mesh_interface: MeshInterface) -> AppState {
    AppState {
        mesh_interface,
        app_settings: Arc::new(Mutex::new(AppSettings {
            get_settings_timeout_seconds: 1,
            signal_data_timeout_seconds: 1,
            route_cost_weight: 0.5,
            route_hops_weight: 0.5,
            ad_hoc_telemetry_timeout_seconds: 1,
        })),
        updating_next_hops_lock: Arc::new(Mutex::new(())),
        telemetry_cache: Arc::new(Mutex::new(BoundedVecDeque::new(50))),
        live_telemetry_is_enabled: Arc::new(AtomicBool::new(false)),
    }
}

pub fn setup_test() -> (MockMesh, TestServer) {
    let mock_mesh = MockMesh::new();
    let app = init_app(create_app_state(mock_mesh.clone_interface()));
    let server = TestServer::new(app).expect("Failed to create test server");

    (mock_mesh, server)
}
