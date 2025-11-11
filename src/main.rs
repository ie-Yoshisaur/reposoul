use reposouls::events::{NotificationEvent, run_event_checker};
use reposouls::gui;
use std::error::Error;
use std::sync::mpsc;
use std::thread;
use tokio::runtime::Runtime;

fn main() -> Result<(), Box<dyn Error>> {
    let (image_sender, image_receiver) = mpsc::channel::<NotificationEvent>();

    thread::spawn(move || {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            run_event_checker(image_sender).await;
        });
    });

    if let Err(e) = gui::run_gui(image_receiver) {
        eprintln!("GUI Error: {}", e);
    }

    Ok(())
}
