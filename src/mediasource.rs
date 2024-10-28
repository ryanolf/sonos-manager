use super::{
    metadata::{apple_uri_and_metadata, spotify_uri_and_metadata},
    Error, Result, SpeakerData,
};
use sonor::utils::escape_str_pcdata;
use sonor::Speaker;

#[derive(Debug)]
/// Definitions for media that can be played and queued.
pub enum MediaSource {
    Apple(String),
    Spotify(String),
    SonosPlaylist(String),
    SonosFavorite(String),
}

use MediaSource::*;
impl MediaSource {
    async fn get_uri_and_metadata(&self, speaker: &Speaker) -> Option<(String, String)> {
        match self {
            Apple(item) => apple_uri_and_metadata(item),
            Spotify(item) => spotify_uri_and_metadata(item),
            SonosPlaylist(item) => {
                let playlists = speaker.browse("SQ:", 0, 0).await.ok()?;
                let playlist = playlists
                    .iter()
                    .find(|&p| p.title().eq_ignore_ascii_case(item))?;
                log::debug!("Found playlist {}", playlist.title());
                Some((playlist.uri()?.into(), "".into()))
            }
            SonosFavorite(item) => {
                let favorites = speaker.browse("FV:2", 0, 0).await.ok()?;
                let favorite = favorites
                    .iter()
                    .find(|&f| f.title().eq_ignore_ascii_case(item))?;
                log::debug!("Found favorite {:?}", favorite);
                Some((favorite.uri()?.into(), favorite.metadata()?.into()))
            }
        }
    }

    /// Add the media to the end of the queue.
    pub(crate) async fn queue_as_next(&self, coordinator_data: &SpeakerData) -> Result<()> {
        let speaker = &coordinator_data.speaker;
        let cur_track_no = coordinator_data
            .get_current_track_no()
            .await
            .map_err(|err| {
                log::warn!("Error determining track number: {}", err);
                err
            })
            .unwrap_or(0);
        let (uri, metadata) = self
            .get_uri_and_metadata(speaker)
            .await
            .ok_or(Error::ContentNotFound)?;
        speaker
            .queue_next(&uri, &escape_str_pcdata(&metadata), Some(cur_track_no + 1))
            .await?;
        Ok(())
    }
    /// Replace what is playing with this
    pub(crate) async fn play_now(&self, coordinator_data: &SpeakerData) -> Result<()> {
        let coordinator = &coordinator_data.speaker;
        let (uri, metadata) = self
            .get_uri_and_metadata(coordinator)
            .await
            .ok_or(Error::ContentNotFound)?;
        coordinator.clear_queue().await?;
        coordinator
            .queue_next(&uri, &escape_str_pcdata(&metadata), Some(1))
            .await?;
        // Turn on queue mode
        let queue_uri = format!("x-rincon-queue:{}#0", coordinator.uuid());
        coordinator.set_transport_uri(&queue_uri, "").await?;
        coordinator.play().await.map_err(Error::from)
    }
}
