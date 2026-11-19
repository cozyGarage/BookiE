use std::io::Write;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout().lock();
    let mut err = std::io::stderr().lock();
    let code = bookie_audit::run(&arguments, None, &mut out, &mut err).await;
    let _ = out.flush();
    std::process::exit(code);
}
