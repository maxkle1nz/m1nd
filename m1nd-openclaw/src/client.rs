use clap::Parser;
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

#[derive(Parser, Debug)]
#[command(
    name = "m1nd-openclaw-client",
    about = "CLI client for the native m1nd OpenClaw bridge"
)]
struct Cli {
    #[arg(long, default_value = "/tmp/m1nd-openclaw.sock")]
    socket: String,

    #[arg(long)]
    result_only: bool,

    tool: String,

    #[arg(default_value = "{}")]
    args: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let args: Value = serde_json::from_str(&cli.args)?;

    let mut stream = UnixStream::connect(&cli.socket)?;
    let request = serde_json::json!({
        "id": "cli",
        "tool": cli.tool,
        "arguments": args,
    });

    let encoded = serde_json::to_string(&request)?;
    stream.write_all(encoded.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    let response: Value = serde_json::from_str(line.trim())?;
    if cli.result_only {
        println!(
            "{}",
            serde_json::to_string_pretty(response.get("result").unwrap_or(&Value::Null))?
        );
    } else {
        println!("{}", serde_json::to_string_pretty(&response)?);
    }
    Ok(())
}
