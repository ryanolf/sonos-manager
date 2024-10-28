use sonor::{discover_one, Error};
use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    let speaker = discover_one(Duration::from_secs(5)).await?;
    println!("Got speaker: {}", speaker.name());

    let content = speaker.browse("SQ:", 0, 0).await?;
    println!("Content: {:?}", content);
    Ok(())
}
