use clap::Parser;
use env_logger::{Builder, Target};
use log::{debug, error, info, trace};
use mcproto_rs::{
    protocol::{ClientHandshake, ClientLogin, ClientPlay},
    Conn, Packet,
};
use miette::{Diagnostic, IntoDiagnostic, Result};
use std::{
    env,
    io::{self, Write},
};
use thiserror::Error;
use tokio::net::TcpStream;

#[derive(Parser)]
#[clap(
    name = "MineCLI",
    version = "0.1.0",
    author = "walker84837 (github.com/walker84837)",
    about = "CLI client for chatting and sending commands in Minecraft servers"
)]
struct Args {
    /// The Minecraft server to connect to (address:port)
    #[clap(required = true)]
    server: String,

    /// The username to use for logging in
    #[clap(short, long, required = true)]
    username: String,

    /// Enable verbose logging
    #[clap(short, long)]
    verbose: bool,
}

#[derive(Error, Debug, Diagnostic)]
enum MineCLIError {
    #[error("Invalid server address format.")]
    #[diagnostic(code(minecli::invalid_address))]
    InvalidAddress,

    #[error("Failed to parse port: {0}")]
    #[diagnostic(code(minecli::port_parse_error))]
    PortParseError(#[source] std::num::ParseIntError),

    #[error("Disconnected during login: {0}")]
    #[diagnostic(code(minecli::disconnected_during_login))]
    Disconnected(String),

    #[error(transparent)]
    #[diagnostic(code(minecli::io_error))]
    Io(#[from] io::Error),

    #[error(transparent)]
    #[diagnostic(code(minecli::other_error))]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

async fn connect_to_minecraft(
    server: &str,
    username: &str,
) -> Result<Conn<ClientPlay>, MineCLIError> {
    let address = server.split(':').collect::<Vec<_>>();
    let host = address.get(0).ok_or(MineCLIError::InvalidAddress)?;
    let port = address
        .get(1)
        .unwrap_or(&"25565")
        .parse::<u16>()
        .map_err(MineCLIError::PortParseError)?;
    info!("Connecting to Minecraft server {}:{}...", host, port);

    // Connect to the server
    let stream = TcpStream::connect((host, port)).await.into_diagnostic()?;
    let mut conn = Conn::new(stream);

    // Handshake
    conn.send_packet(&ClientHandshake::new(host.to_string(), port, 2).to_packet())
        .await
        .into_diagnostic()?;
    conn.send_packet(
        &ClientLogin::Start {
            name: username.to_string(),
        }
        .to_packet(),
    )
    .await
    .into_diagnostic()?;
    info!("Sent handshake and login packets.");

    // Await login response
    while let Some(packet) = conn
        .receive_packet::<ClientPlay>()
        .await
        .into_diagnostic()?
    {
        match packet {
            Packet::LoginSuccess(_) => {
                info!("Successfully logged in as {}", username);
                break;
            }
            Packet::Disconnect(d) => {
                error!("Disconnected: {}", d.reason);
                return Err(MineCLIError::Disconnected(d.reason));
            }
            _ => debug!("Received unexpected packet: {:?}", packet),
        }
    }

    Ok(conn)
}

async fn repl(mut conn: Conn<ClientPlay>) -> Result<()> {
    loop {
        let mut command = String::new();
        print!("Enter command (or 'exit' to quit): ");
        io::stdout().flush().into_diagnostic()?;
        io::stdin().read_line(&mut command).into_diagnostic()?;
        let command = command.trim();

        if command.eq_ignore_ascii_case("exit") {
            info!("Exiting REPL.");
            break;
        }

        // Send command to the server
        if let Err(e) = conn
            .send_packet(
                &ClientPlay::ChatMessage {
                    message: command.to_string(),
                }
                .to_packet(),
            )
            .await
        {
            error!("Failed to send command: {}", e);
        } else {
            info!("Command sent: {}", command);
        }

        // Receive and display server response
        if let Some(packet) = conn.receive_packet::<ClientPlay>().await.ok().flatten() {
            if let Packet::ChatMessage(chat) = packet {
                println!("Server: {}", chat.message);
            }
        }
    }
    Ok(())
}

fn init_logger(verbose: bool) {
    let mut builder = Builder::from_default_env();
    builder.target(Target::Stdout);

    if verbose {
        builder.filter_level(log::LevelFilter::Debug);
    } else {
        builder.filter_level(log::LevelFilter::Info);
    }

    builder.init();
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logger
    init_logger(args.verbose);

    println!("Welcome to MineCLI!");
    info!("Connecting to server {} as {}", args.server, args.username);

    let conn = connect_to_minecraft(&args.server, &args.username).await?;
    repl(conn).await?;

    Ok(())
}
