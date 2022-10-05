use lib;
use log::{info, warn};

fn main() {
    env_logger::init();

    loop {
        match lib::detectsend_loop() {
            Ok(()) => break,
            Err(e) => {
                warn!("Error: {:?}", e,);
                info!("Retrying in {:?}", lib::DETECT_RETRY_DELAY);
            }
        }
        std::thread::sleep(lib::DETECT_RETRY_DELAY);
    }
}
