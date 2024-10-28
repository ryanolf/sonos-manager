#![allow(unused_imports)]
use std::time::Duration;

use sonos_manager::{Error, Manager, MediaSource::*};
use tokio::time::sleep;
// const ZONE: &str = "Sonos Roam";
const ZONE: &str = "Master Bedroom";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    simple_logger::init_with_level(log::Level::Debug).unwrap();

    let manager = Manager::new_with_roomname(ZONE).await?;
    println!("Got manager");
    let zone = manager.get_zone(ZONE.into()).await?;
    // let snapshot = zone.take_snapshot().await?;
    // zone.clear_queue().await?;
    // zone.play_now(Apple("librarytrack:a.1442979904".into())).await?;
    // zone.next_track().await?;
    // zone.play_now(Apple("track:1025212410".into())).await?;
    // zone.play_now(Spotify("track:4LI1ykYGFCcXPWkrpcU7hn".into())).await?;
    zone.play_now(Spotify("album:7DuJYWu66RPdcekF5TuZ7w".into()))
        .await?;
    zone.play_now(Spotify("playlist:37i9dQZF1DX889U0CL85jj".into()))
        .await?;
    // zone.play_now(Apple("album:1439398162".into())).await?;
    // zone.play_now(SonosFavorite("New York Rhapsody".into())).await?;
    // sleep(Duration::from_secs(10)).await;
    // zone.set_play_mode(sonor::RepeatMode::One, true).await?;
    // zone.play_or_pause().await?;
    // sleep(Duration::from_secs(5)).await;
    // zone.next_track().await?;
    // sleep(Duration::from_secs(5)).await;
    // zone.pause().await?;
    // zone.previous_track().await?;
    // zone.set_shuffle(true).await?;
    // zone.seek_track(seconds)
    // zone.set_play_mode(sonor::RepeatMode::None, false).await?;
    // zone.play_now(SonosPlaylist("Cars 1, 2, 3".into())).await?;
    // zone.pause().await?;
    // zone.apply_snapshot(snapshot).await?;
    // zone.seek_rel_track(5).await?;
    Ok(())
}
