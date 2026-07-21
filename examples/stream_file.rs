//! Drive an FSM from a streamed file: read the input line by line and feed each
//! line as an event; the handler streams the transformed line to an output file.
//! Handlers are ordinary `async fn`, so they can do streaming I/O directly.
//!
//! Run: `cargo run --example stream_file`

use std::error::Error;

use statecraft::fsm;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};

struct Ctx {
    out: BufWriter<File>,
    lines: usize,
}

#[fsm(initial = Reading)]
impl Transform {
    type Context = Ctx;
    type Error = std::io::Error;

    // Payload event carrying one input line; streamed out uppercased.
    #[on(state = Reading, event = Line(String), next = Reading)]
    async fn on_line(&mut self, line: String) -> Result<(), std::io::Error> {
        self.context.lines += 1;
        self.context
            .out
            .write_all(line.to_uppercase().as_bytes())
            .await?;
        self.context.out.write_all(b"\n").await?;
        Ok(())
    }

    #[on(state = Reading, event = Eof, next = Done)]
    async fn on_eof(&mut self) -> Result<(), std::io::Error> {
        self.context.out.flush().await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let dir = std::env::temp_dir();
    let in_path = dir.join("statecraft_stream_in.txt");
    let out_path = dir.join("statecraft_stream_out.txt");
    tokio::fs::write(&in_path, "hello\nworld\nfrom\nstatecraft\n").await?;

    let out = BufWriter::new(File::create(&out_path).await?);
    let mut fsm = Transform::new(Ctx { out, lines: 0 });

    // Drive the FSM from the file, one streamed line at a time.
    let mut reader = BufReader::new(File::open(&in_path).await?).lines();
    while let Some(line) = reader.next_line().await? {
        fsm.apply(TransformEvent::Line(line)).await?;
    }
    fsm.apply(TransformEvent::Eof).await?;

    println!(
        "state={:?}, transformed {} lines -> {}",
        fsm.state(),
        fsm.context.lines,
        out_path.display()
    );
    println!(
        "--- output ---\n{}",
        tokio::fs::read_to_string(&out_path).await?
    );
    Ok(())
}
