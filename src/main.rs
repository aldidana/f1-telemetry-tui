use async_std::io::Error;
use async_std::task;
use crossbeam_channel::unbounded;
use f1_telemetry_client::{packet::Packet, Telemetry};

mod app;

#[async_std::main]
async fn main() -> Result<(), Error> {
    let client = Telemetry::new("192.168.1.9", 20777).await.unwrap();

    let (tx, rx) = unbounded();
    let rx_clone = rx.clone();
    let mut app = app::App::new();
    app.start(rx_clone)?;

    loop {
        match client.next().await {
            Ok(p) => match p {
                Packet::F12020(result) => {
                    let sender = tx.clone();
                    task::spawn(async move {
                        match sender.send(result) {
                            Ok(_) => {}
                            Err(e) => eprintln!("Error send channel {}", e),
                        }
                    });
                }
                _ => unimplemented!(),
            },
            Err(e) => eprintln!("Error when receive UDP packet {}", e),
        }
    }
}
