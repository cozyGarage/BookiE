use std::io::{BufRead, IsTerminal, Write};

struct Tty {
    input: std::io::BufReader<std::fs::File>,
    output: std::fs::File,
}

impl Tty {
    fn open() -> Option<Self> {
        if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
            return None;
        }
        let input = std::fs::OpenOptions::new().read(true).open("/dev/tty").ok()?;
        let output = std::fs::OpenOptions::new().write(true).open("/dev/tty").ok()?;
        Some(Self {
            input: std::io::BufReader::new(input),
            output,
        })
    }
}

impl bookie_audit::Terminal for Tty {
    fn ask(&mut self, prompt: &str) -> Option<String> {
        self.output.write_all(prompt.as_bytes()).ok()?;
        self.output.flush().ok()?;
        let mut line = String::new();
        self.input.read_line(&mut line).ok()?;
        Some(line)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let mut tty = Tty::open();
    let terminal = tty.as_mut().map(|tty| tty as &mut dyn bookie_audit::Terminal);
    let mut out = std::io::stdout().lock();
    let mut err = std::io::stderr().lock();
    let code = bookie_audit::run(&arguments, None, terminal, &mut out, &mut err).await;
    let _ = out.flush();
    std::process::exit(code);
}
