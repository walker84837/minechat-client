use clap::Parser;
use directories::ProjectDirs;
use env_logger::{Builder, Target};
use log::{debug, info};
use miette::Result;
use minechat_protocol::{
    packets::{self, MineChatError, receive_message, send_message},
    protocol::*,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    path::PathBuf,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, BufReader},
    net::TcpStream,
    signal,
};

#[derive(Parser)]
#[clap(
    name = "MineChat",
    version = "0.1.0",
    author = "walker84837",
    about = "CLI client for MineChat"
)]
struct Args {
    /// The MineChat server address (host:port)
    #[clap(short, long, required = true)]
    server: String,

    /// Link account using the provided code
    #[clap(long)]
    link: Option<String>,

    /// Enable verbose logging
    #[clap(short, long)]
    verbose: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct ServerConfig {
    servers: Vec<ServerEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ServerEntry {
    address: String,
    uuid: String,
}

fn config_path() -> Result<PathBuf, MineChatError> {
    let proj_dirs = ProjectDirs::from("", "", "minechat")
        .ok_or(MineChatError::ConfigError("Can't get config dir".into()))?;
    let config_dir = proj_dirs.config_dir();
    fs::create_dir_all(config_dir)?;
    Ok(config_dir.join("servers.json"))
}

fn load_config() -> Result<ServerConfig, MineChatError> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(ServerConfig {
            servers: Vec::new(),
        });
    }
    let file = File::open(path)?;
    Ok(serde_json::from_reader(file)?)
}

fn save_config(config: &ServerConfig) -> Result<(), MineChatError> {
    let path = config_path()?;
    let file = File::create(path)?;
    Ok(serde_json::to_writer_pretty(file, config)?)
}

async fn set_link(server_addr: &str, code: &str) -> Result<(), MineChatError> {
    let (client_uuid, _link_code) = packets::handle_link(server_addr, code).await?;

    info!("Linked successfully");
    let mut config = load_config()?;
    config.servers.retain(|e| e.address != server_addr);
    config.servers.push(ServerEntry {
        address: server_addr.to_string(),
        uuid: client_uuid,
    });
    save_config(&config)?;
    Ok(())
}

async fn handle_connect(server_addr: &str) -> Result<(), MineChatError> {
    let config = load_config()?;
    let entry = config
        .servers
        .iter()
        .find(|e| e.address == server_addr)
        .ok_or(MineChatError::ServerNotLinked)?;

    let mut stream = TcpStream::connect(server_addr).await?;
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);

    send_message(
        &mut writer,
        &MineChatMessage::Auth {
            payload: AuthPayload {
                client_uuid: entry.uuid.clone(),
                link_code: String::new(),
            },
        },
    )
    .await?;

    match receive_message(&mut reader).await? {
        MineChatMessage::AuthAck { payload } => {
            if payload.status == "success" {
                info!("Connected: {}", payload.message);
                // Pass the split reader and writer to repl
                let (reader, writer) = stream.into_split();
                repl(BufReader::new(reader), writer).await
            } else {
                Err(MineChatError::AuthFailed(payload.message))
            }
        }
        _ => Err(MineChatError::AuthFailed("Unexpected response".into())),
    }
}

async fn repl<R, W>(mut reader: R, mut writer: W) -> Result<(), MineChatError>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut stdin = BufReader::new(tokio::io::stdin());
    let mut buffer = String::new();
    let mut msg_buffer = String::new();

    loop {
        tokio::select! {
            result = reader.read_line(&mut msg_buffer) => {
                match result {
                    Ok(0) => return Ok(()),
                    Ok(_) => {
                        if let Ok(msg) = serde_json::from_str::<MineChatMessage>(&msg_buffer) {
                            match msg {
                                MineChatMessage::Broadcast { payload } => {
                                    println!("[{}] {}", payload.from, payload.message);
                                }
                                MineChatMessage::Disconnect { payload } => {
                                    println!("Disconnected: {}", payload.reason);
                                    return Ok(());
                                }
                                _ => debug!("Received message: {:?}", msg),
                            }
                        }
                        msg_buffer.clear();
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            result = stdin.read_line(&mut buffer) => {
                let n = result?;
                if n == 0 {
                    send_message(&mut writer, &MineChatMessage::Disconnect {
                        payload: DisconnectPayload { reason: "Client exit".into() }
                    }).await?;
                    return Ok(());
                }
                let input = buffer.trim().to_string();
                if input == "/exit" {
                    send_message(&mut writer, &MineChatMessage::Disconnect {
                        payload: DisconnectPayload { reason: "Client exit".into() }
                    }).await?;
                    return Ok(());
                }
                send_message(&mut writer, &MineChatMessage::Chat {
                    payload: ChatPayload { message: input }
                }).await?;
                buffer.clear();
            }
            _ = signal::ctrl_c() => {
                send_message(&mut writer, &MineChatMessage::Disconnect {
                    payload: DisconnectPayload { reason: "Client exit".into() }
                }).await?;
                return Ok(());
            }
        }
    }
}

fn init_logger(verbose: bool) {
    let mut builder = Builder::from_default_env();
    builder.target(Target::Stdout);
    builder.filter_level(if verbose {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    });
    builder.init();
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    init_logger(args.verbose);

    if let Some(code) = args.link {
        set_link(&args.server, &code).await
    } else {
        handle_connect(&args.server).await
    }
    .map_err(|e| miette::Report::new(e))?;

    Ok(())
}
