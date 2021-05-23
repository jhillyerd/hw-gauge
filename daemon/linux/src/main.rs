use lib;

fn main() {
    loop {
        match lib::detectsend_loop() {
            Ok(()) => break,
            Err(e) => {
                eprintln!(
                    "Error: {:?}\nRetrying in {:?}...",
                    e,
                    lib::DETECT_RETRY_DELAY
                );
            }
        }
        std::thread::sleep(lib::DETECT_RETRY_DELAY);
    }
}
