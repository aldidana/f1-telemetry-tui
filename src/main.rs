use async_std::io::Error;
use async_std::task;
use crossbeam_channel::unbounded;
use f1_telemetry_client::{packet::Packet, Telemetry};

mod app;

#[async_std::main]
async fn main() -> Result<(), Error> {
    let ip_address = std::env::args().nth(1).expect("No IP Address given");
    let port = std::env::args().nth(2).expect("No Port given");
    let port = port.parse().expect("Port must be number");

    let client = Telemetry::new(ip_address.as_str(), port).await.unwrap();

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
