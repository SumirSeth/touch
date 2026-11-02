use gestures_engine::{load_config, Engine};
fn main() -> std::io::Result<()> {
    let (config, warning) = load_config();
    if let Some(warning) = warning {
        eprintln!("{warning}");
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let engine = Engine::start(config, Some(tx))?;
    let printer = std::thread::spawn(move || {
        for event in rx {
            println!("{event:?}");
        }
    });
    println!("Engine running with saved bindings. Press Enter to stop.");
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    engine.stop();
    let _ = printer.join();
    Ok(())
}
