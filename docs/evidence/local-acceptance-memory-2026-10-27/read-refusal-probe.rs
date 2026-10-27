use std::io::Read;
fn rss() -> String {
    std::fs::read_to_string("/proc/self/status").unwrap().lines().find(|l| l.starts_with("VmRSS:")).unwrap().to_owned()
}
fn main() {
    let path = std::env::args().nth(1).unwrap();
    println!("before {}", rss());
    for cycle in 1..=20 {
        let path = path.clone();
        std::thread::spawn(move || {
            let file = std::fs::File::open(path).unwrap();
            let mut bytes = Vec::new();
            file.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes).unwrap();
            let text = String::from_utf8(bytes).unwrap();
            assert!(text.lines().any(|line| line.len() > 256 * 1024));
        }).join().unwrap();
        println!("{cycle} {}", rss());
    }
}
