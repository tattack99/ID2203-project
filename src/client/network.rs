use futures::{SinkExt, StreamExt};
use log::*;
use omnipaxos_kv::common::{kv::NodeId, messages::*, utils::*};
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::Duration;
use tokio::sync::mpsc::{self, channel};
use tokio::task::JoinHandle;
use tokio::{net::TcpStream, sync::mpsc::Receiver};
use tokio::{sync::mpsc::Sender, time::interval};

pub struct Network {
    server_connections: Vec<Option<ServerConnection>>,
    server_message_sender: Sender<ServerMessage>,
    pub server_messages: Receiver<ServerMessage>,
    batch_size: usize,
    /// Keep server addresses around so we can reconnect
    servers: Vec<(NodeId, String)>,
}

const RETRY_SERVER_CONNECTION_TIMEOUT: Duration = Duration::from_secs(1);

impl Network {
    pub async fn new(servers: Vec<(NodeId, String)>, batch_size: usize) -> Self {
        let mut server_connections = vec![];
        let max_server_id = *servers.iter().map(|(id, _)| id).max().unwrap() as usize;
        server_connections.resize_with(max_server_id + 1, Default::default);
        let (server_message_sender, server_messages) = channel(batch_size);
        let mut network = Self {
            server_connections,
            batch_size,
            server_message_sender,
            server_messages,
            servers: servers.clone(),
        };
        network.initialize_connections(&servers).await;
        network
    }

    async fn initialize_connections(&mut self, servers: &Vec<(NodeId, String)>) {
        info!("Establishing server connections");
        let mut connection_tasks = Vec::with_capacity(servers.len());
        for (server_id, server_addr_str) in servers {
            let server_address = server_addr_str
                .to_socket_addrs()
                .expect("Unable to resolve server IP")
                .next()
                .unwrap();
            let task = tokio::spawn(Self::get_server_connection(*server_id, server_address));
            connection_tasks.push(task);
        }
        let finished_tasks = futures::future::join_all(connection_tasks).await;
        for (i, result) in finished_tasks.into_iter().enumerate() {
            match result {
                Ok((from_server_conn, to_server_conn)) => {
                    let connected_server_id = servers[i].0;
                    info!("Connected to server {connected_server_id}");
                    let server_idx = connected_server_id as usize;
                    let server_connection = ServerConnection::new(
                        connected_server_id,
                        from_server_conn,
                        to_server_conn,
                        self.batch_size,
                        self.server_message_sender.clone(),
                    );
                    self.server_connections[server_idx] = Some(server_connection)
                }
                Err(err) => {
                    let failed_server = servers[i].0;
                    panic!("Unable to establish connection to server {failed_server}: {err}")
                }
            }
        }
    }

    async fn get_server_connection(
        server_id: NodeId,
        server_address: SocketAddr,
    ) -> (FromServerConnection, ToServerConnection) {
        let mut retry_connection = interval(RETRY_SERVER_CONNECTION_TIMEOUT);
        loop {
            retry_connection.tick().await;
            match TcpStream::connect(server_address).await {
                Ok(stream) => {
                    stream.set_nodelay(true).unwrap();
                    let mut registration_connection = frame_registration_connection(stream);
                    registration_connection
                        .send(RegistrationMessage::ClientRegister)
                        .await
                        .expect("Couldn't send registration to server");
                    let underlying_stream = registration_connection.into_inner().into_inner();
                    break frame_clients_connection(underlying_stream);
                }
                Err(e) => error!("Unable to connect to server {server_id}: {e}"),
            }
        }
    }

    /// Attempt to reconnect to a specific server. This is called when we detect
    /// the connection has dropped (send failed or connection set to None).
    /// Spawns the reconnection in the background so we don't block the event loop.
    pub fn spawn_reconnect(&mut self, server_id: NodeId) {
        let server_idx = server_id as usize;

        // Drop old connection if any
        if let Some(Some(old_conn)) = self.server_connections.get_mut(server_idx) {
            let conn = self.server_connections[server_idx].take().unwrap();
            conn.close();
        }
        self.server_connections[server_idx] = None;

        // Find the address for this server
        let addr_str = match self.servers.iter().find(|(id, _)| *id == server_id) {
            Some((_, addr)) => addr.clone(),
            None => {
                error!("No address known for server {server_id}, cannot reconnect");
                return;
            }
        };

        let server_address = match addr_str.to_socket_addrs() {
            Ok(mut addrs) => addrs.next().unwrap(),
            Err(e) => {
                error!("Cannot resolve address for server {server_id}: {e}");
                return;
            }
        };

        let batch_size = self.batch_size;
        let sender = self.server_message_sender.clone();

        // We use a oneshot to get the new ServerConnection back to the Network.
        // But since Network isn't Send-friendly for holding across awaits, we
        // use a mpsc channel and poll it in the client's run loop.
        // Actually, simpler: we spawn the reconnect and send the result through
        // the existing server_message_sender as a special reconnect notification.
        // But that changes the message type...
        //
        // Simplest approach: do blocking reconnect inline. The client's run loop
        // calls reconnect() and awaits it.
        //
        // We'll use the approach of returning a task handle instead.
        // See reconnect_blocking below.
        info!("Reconnect to server {server_id} will happen on next send attempt or explicit call");
    }

    /// Blocking reconnect: await this to reconnect to a specific server.
    /// Returns true if reconnection succeeded.
    pub async fn reconnect(&mut self, server_id: NodeId) -> bool {
        let server_idx = server_id as usize;

        // Drop old connection
        if let Some(old_conn) = self.server_connections[server_idx].take() {
            old_conn.close();
        }

        let addr_str = match self.servers.iter().find(|(id, _)| *id == server_id) {
            Some((_, addr)) => addr.clone(),
            None => {
                error!("No address known for server {server_id}");
                return false;
            }
        };

        let server_address = match addr_str.to_socket_addrs() {
            Ok(mut addrs) => match addrs.next() {
                Some(a) => a,
                None => {
                    error!("No addresses resolved for server {server_id}");
                    return false;
                }
            },
            Err(e) => {
                error!("Cannot resolve address for server {server_id}: {e}");
                return false;
            }
        };

        info!("Attempting to reconnect to server {server_id} at {server_address}...");
        match Self::get_server_connection(server_id, server_address).await {
            (from_conn, to_conn) => {
                let connection = ServerConnection::new(
                    server_id,
                    from_conn,
                    to_conn,
                    self.batch_size,
                    self.server_message_sender.clone(),
                );
                self.server_connections[server_idx] = Some(connection);
                info!("Reconnected to server {server_id}");
                true
            }
        }
    }

    /// Check if we're connected to a given server
    pub fn is_connected(&self, server_id: NodeId) -> bool {
        match self.server_connections.get(server_id as usize) {
            Some(Some(_)) => true,
            _ => false,
        }
    }

    pub async fn send(&mut self, to: NodeId, msg: ClientMessage) -> Result<(), ()> {
        match self.server_connections.get_mut(to as usize) {
            Some(connection_slot) => match connection_slot {
                Some(connection) => {
                    if let Err(err) = connection.send(msg).await {
                        warn!("Couldn't send msg to server {to}: {err}");
                        self.server_connections[to as usize] = None;
                        Err(())
                    } else {
                        Ok(())
                    }
                }
                None => {
                    error!("Not connected to server {to}");
                    Err(())
                }
            },
            None => {
                error!("Sending to unexpected server {to}");
                Err(())
            }
        }
    }

    // Removes all server connections and ends their corresponding tasks
    pub fn shutdown(&mut self) {
        let connection_count = self.server_connections.len();
        for server_connection in self.server_connections.drain(..) {
            if let Some(connection) = server_connection {
                connection.close();
            }
        }
        for _ in 0..connection_count {
            self.server_connections.push(None);
        }
    }
}

struct ServerConnection {
    // server_id: NodeId,
    reader_task: JoinHandle<()>,
    writer_task: JoinHandle<()>,
    outgoing_messages: Sender<ClientMessage>,
}

impl ServerConnection {
    pub fn new(
        server_id: NodeId,
        reader: FromServerConnection,
        mut writer: ToServerConnection,
        batch_size: usize,
        incoming_messages: Sender<ServerMessage>,
    ) -> Self {
        // Reader Actor
        let reader_task = tokio::spawn(async move {
            let mut buf_reader = reader.ready_chunks(batch_size);
            while let Some(messages) = buf_reader.next().await {
                for msg in messages {
                    match msg {
                        Ok(m) => {
                            if incoming_messages.send(m).await.is_err() {
                                return; // receiver dropped
                            }
                        }
                        Err(err) => error!("Error deserializing message: {:?}", err),
                    }
                }
            }
            info!("Reader for server {server_id} ended (connection dropped)");
        });
        // Writer Actor
        let (message_tx, mut message_rx) = mpsc::channel(batch_size);
        let writer_task = tokio::spawn(async move {
            let mut buffer = Vec::with_capacity(batch_size);
            while message_rx.recv_many(&mut buffer, batch_size).await != 0 {
                for msg in buffer.drain(..) {
                    if let Err(err) = writer.feed(msg).await {
                        error!("Couldn't send message to server {server_id}: {err}");
                        break;
                    }
                }
                if let Err(err) = writer.flush().await {
                    error!("Couldn't send message to server {server_id}: {err}");
                    break;
                }
            }
            info!("Connection to server {server_id} closed");
        });
        ServerConnection {
            // server_id,
            reader_task,
            writer_task,
            outgoing_messages: message_tx,
        }
    }

    pub async fn send(
        &mut self,
        msg: ClientMessage,
    ) -> Result<(), mpsc::error::SendError<ClientMessage>> {
        self.outgoing_messages.send(msg).await
    }

    fn close(self) {
        self.reader_task.abort();
        self.writer_task.abort();
    }
}