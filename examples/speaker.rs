use sonor::{find, Error};
use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    let speaker = find("Master Bedroom", Duration::from_secs(5))
        .await?
        .unwrap();
    println!("Got speaker: {}", speaker.name());

    let content = speaker.track().await?;
    println!("Content: {:#?}", content);
    Ok(())
}
