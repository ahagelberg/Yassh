use crate::config::{AuthMethod, BackspaceKey, LineEnding, ResizeMethod, SessionConfig};
use crate::debug;
use anyhow::{Context, Result};
use ssh2::{Channel, Session};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// Connection constants
const READ_BUFFER_SIZE: usize = 4096;
const CHANNEL_CHECK_INTERVAL_MS: u64 = 10;

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

pub enum SshEvent {
    Connected,
    Data(Vec<u8>),
    Disconnected { natural: bool },
    Error(String),
}

pub enum SshCommand {
    Write(Vec<u8>),
    Disconnect,
    Resize { cols: u32, rows: u32 },
}

pub struct SshConnection {
    state: Arc<Mutex<ConnectionState>>,
    event_rx: Receiver<SshEvent>,
    command_tx: Sender<SshCommand>,
    config: SessionConfig,
}

impl SshConnection {
    pub fn new(config: SessionConfig) -> Self {
        let (event_tx, event_rx) = mpsc::channel();
        let (command_tx, command_rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(ConnectionState::Disconnected));
        let connection = Self {
            state: state.clone(),
            event_rx,
            command_tx,
            config: config.clone(),
        };
        let state_clone = state.clone();
        thread::spawn(move || {
            Self::connection_thread(config, state_clone, event_tx, command_rx);
        });
        connection
    }

    fn connection_thread(
        config: SessionConfig,
        state: Arc<Mutex<ConnectionState>>,
        event_tx: Sender<SshEvent>,
        command_rx: Receiver<SshCommand>,
    ) {
        debug::log(&format!("[SSH {}] Connecting to {}:{}", config.id, config.host, config.port));
        *state.lock().unwrap() = ConnectionState::Connecting;
        let result = Self::establish_connection(&config);
        match result {
            Ok((session, mut channel)) => {
                debug::log(&format!("[SSH {}] Connected", config.id));
                *state.lock().unwrap() = ConnectionState::Connected;
                let _ = event_tx.send(SshEvent::Connected);
                Self::run_session(&config, session, &mut channel, &state, &event_tx, &command_rx);
            }
            Err(e) => {
                let error_msg = format!("{:#}", e);
                debug::log(&format!("[SSH {}] Error: {}", config.id, error_msg));
                *state.lock().unwrap() = ConnectionState::Error(error_msg.clone());
                let _ = event_tx.send(SshEvent::Error(error_msg));
            }
        }
        debug::log(&format!("[SSH {}] Connection thread ended", config.id));
        *state.lock().unwrap() = ConnectionState::Disconnected;
    }

    fn establish_connection(config: &SessionConfig) -> Result<(Session, Channel)> {
        let address = format!("{}:{}", config.host, config.port);
        let timeout = config.timeout;
        let tcp = TcpStream::connect_timeout(
            &address.parse().context("Invalid address")?,
            timeout,
        )
        .context("Failed to connect to host")?;
        tcp.set_read_timeout(Some(Duration::from_millis(CHANNEL_CHECK_INTERVAL_MS)))?;
        let mut session = Session::new().context("Failed to create SSH session")?;
        session.set_tcp_stream(tcp);
        session.handshake().context("SSH handshake failed")?;
        match config.auth_method {
            AuthMethod::Password => {
                let password = config.password.as_deref().unwrap_or("");
                session
                    .userauth_password(&config.username, password)
                    .context("Password authentication failed")?;
            }
            AuthMethod::PrivateKey => {
                let key_path = config
                    .private_key_path
                    .as_ref()
                    .context("Private key path not specified")?;
                session
                    .userauth_pubkey_file(
                        &config.username,
                        None,
                        key_path,
                        config.password.as_deref(),
                    )
                    .context("Public key authentication failed")?;
            }
        }
        if !session.authenticated() {
            anyhow::bail!("Authentication failed");
        }
        if config.compression {
            // Compression is handled automatically by libssh2
        }
        let mut channel = session.channel_session().context("Failed to open channel")?;
        channel.request_pty("xterm-256color", None, None)?;
        channel.shell().context("Failed to start shell")?;
        if let Some(screen_name) = &config.screen_session {
            let screen_cmd = format!(
                "screen -x {} || screen -S {}\n",
                screen_name, screen_name
            );
            channel.write_all(screen_cmd.as_bytes())?;
        }
        Ok((session, channel))
    }

    fn run_session(
        config: &SessionConfig,
        session: Session,
        channel: &mut Channel,
        _state: &Arc<Mutex<ConnectionState>>,
        event_tx: &Sender<SshEvent>,
        command_rx: &Receiver<SshCommand>,
    ) {
        // Set non-blocking mode so reads don't block the command processing
        session.set_blocking(false);
        
        let mut read_buffer = [0u8; READ_BUFFER_SIZE];
        let keepalive_interval = if config.keep_alive {
            Some(config.keepalive_interval)
        } else {
            None
        };
        let mut last_keepalive = std::time::Instant::now();
        #[allow(unused_assignments)]
        let mut disconnect_natural = false;
        loop {
            match command_rx.try_recv() {
                Ok(SshCommand::Write(_data)) => {
                    if let Err(e) = channel.write_all(&_data) {
                        debug::log(&format!("[SSH {}] Write error: {:?}", config.id, e));
                        disconnect_natural = false;
                        break;
                    }
                }
                Ok(SshCommand::Disconnect) => {
                    disconnect_natural = true;
                    break;
                }
                Ok(SshCommand::Resize { cols, rows }) => {
                    if let Err(e) = Self::handle_resize(config, channel, cols, rows) {
                        debug::log(&format!("[SSH {}] Resize error: {:?}", config.id, e));
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    disconnect_natural = false;
                    break;
                }
            }
            match channel.read(&mut read_buffer) {
                Ok(0) => {
                    if channel.eof() {
                        debug::log(&format!("[SSH {}] EOF", config.id));
                        disconnect_natural = true;
                        break;
                    }
                }
                Ok(n) => {
                    let data = read_buffer[..n].to_vec();
                    if event_tx.send(SshEvent::Data(data)).is_err() {
                        disconnect_natural = false;
                        break;
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => {
                    debug::log(&format!("[SSH {}] Read error: {:?}", config.id, e));
                    disconnect_natural = false;
                    break;
                }
            }
            if let Some(interval) = keepalive_interval {
                if last_keepalive.elapsed() >= interval {
                    if session.keepalive_send().is_err() {
                        disconnect_natural = false;
                        break;
                    }
                    last_keepalive = std::time::Instant::now();
                }
            }
        }
        let _ = event_tx.send(SshEvent::Disconnected { natural: disconnect_natural });
    }

    pub fn state(&self) -> ConnectionState {
        self.state.lock().unwrap().clone()
    }

    pub fn try_recv(&self) -> Option<SshEvent> {
        self.event_rx.try_recv().ok()
    }

    pub fn send(&self, data: &[u8]) {
        let data = self.convert_line_endings(data);
        let _ = self.command_tx.send(SshCommand::Write(data));
    }

    #[allow(dead_code)]
    pub fn send_key(&self, key: &str) {
        self.send(key.as_bytes());
    }


    pub fn disconnect(&self) {
        let _ = self.command_tx.send(SshCommand::Disconnect);
    }

    pub fn resize_terminal(&self, cols: u32, rows: u32) {
        let _ = self.command_tx.send(SshCommand::Resize { cols, rows });
    }

    fn handle_resize(config: &SessionConfig, channel: &mut Channel, cols: u32, rows: u32) -> Result<()> {
        match config.resize_method {
            ResizeMethod::Ssh => {
                channel.request_pty("xterm-256color", None, Some((cols, rows, 0, 0)))?;
            }
            ResizeMethod::Ansi => {
                let cmd = format!("\x1b[8;{};{}t", rows, cols);
                channel.write_all(cmd.as_bytes())?;
            }
            ResizeMethod::Stty => {
                let cmd = format!("stty cols {} rows {}\n", cols, rows);
                channel.write_all(cmd.as_bytes())?;
            }
            ResizeMethod::XTerm => {
                let cmd = format!("\x1b[7;{};{}t", rows, cols);
                channel.write_all(cmd.as_bytes())?;
            }
            ResizeMethod::None => {}
        }
        Ok(())
    }

    fn convert_line_endings(&self, data: &[u8]) -> Vec<u8> {
        match self.config.line_ending {
            LineEnding::Lf => data.to_vec(),
            LineEnding::CrLf => {
                let mut result = Vec::with_capacity(data.len() * 2);
                for &byte in data {
                    if byte == b'\n' {
                        result.push(b'\r');
                    }
                    result.push(byte);
                }
                result
            }
            LineEnding::Cr => {
                data.iter().map(|&b| if b == b'\n' { b'\r' } else { b }).collect()
            }
        }
    }

    pub fn backspace_sequence(&self) -> &[u8] {
        match self.config.backspace_key {
            BackspaceKey::Del => &[0x7F],
            BackspaceKey::CtrlH => &[0x08],
        }
    }

    #[allow(dead_code)]
    pub fn config(&self) -> &SessionConfig {
        &self.config
    }
}

impl Drop for SshConnection {
    fn drop(&mut self) {
        self.disconnect();
    }
}

