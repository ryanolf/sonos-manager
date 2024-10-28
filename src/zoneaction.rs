use std::convert::TryInto;

use sonor::{RepeatMode, Snapshot, Speaker};

use super::Controller;
use crate::{
    controller::SpeakerData,
    types::{Responder, Response},
    Error, MediaSource, Result,
};

#[derive(Debug)]
pub enum ZoneAction {
    Exists,
    PlayNow(MediaSource),
    QueueAsNext(MediaSource),
    Play,
    Pause,
    PlayPause,
    NextTrack,
    PreviousTrack,
    SeekTime(u32),
    SeekTrack(u32),
    SeekRelTrack(i32),
    SetRepeat(RepeatMode),
    SetShuffle(bool),
    SetCrossfade(bool),
    SetPlayMode(RepeatMode, bool),
    ClearQueue,
    GetQueue,
    TakeSnapshot,
    ApplySnapshot(Snapshot),
    SetRelVolume(i32),
}
use ZoneAction::*;

impl ZoneAction {
    pub(super) async fn handle_action(
        self,
        controller: &Controller,
        tx: Responder,
        name: String,
    ) -> Result<()> {
        macro_rules! data_action {
            ($data:ident.$method:ident($payload:ident: $letmethod:ident) -> $res:ident($returnval:ident) ) => {{
                if let Some($payload) = controller.$letmethod(&name) {
                    log::debug!(
                        "Attempting to {:?} with {:?} in {:?}",
                        stringify!($method),
                        $data,
                        name
                    );
                    match $data.$method($payload).await {
                        Ok($returnval) => {
                            return tx.send(Response::$res($returnval)).or_else(|_| Ok(()))
                        }
                        Err(e) => log::warn!("Error: {}", e),
                    }
                }
                tx.send(Response::NotOk).ok();
            }};
        }
        macro_rules! controller_action {
            ($payload:ident.$method:ident($($data:ident),*) : $letmethod:ident -> $res:ident($returnval:ident) ) => {{
                if let Some($payload) = controller.$letmethod(&name) {
                    log::debug!("Attempting to {:#?} in {}", stringify!($method), name);
                    match $payload.$method($($data),*).await {
                        Ok($returnval) => {
                            return tx.send(Response::$res($returnval)).or_else(|_| Ok(()))
                        }
                        Err(e) => log::warn!("Error: {}", e),
                    }
                }
                tx.send(Response::NotOk).ok();
            }};
        }

        match self {
            PlayNow(media) => {
                data_action!( media.play_now(coordinatordata: get_coordinatordata_for_name) -> Ok(__) )
            }
            QueueAsNext(media) => {
                data_action!( media.queue_as_next(coordinatordata: get_coordinatordata_for_name) -> Ok(__) )
            }
            Play => controller_action!( coordinator.play(): get_coordinator_for_name -> Ok(__) ),
            Pause => controller_action!( coordinator.pause(): get_coordinator_for_name -> Ok(__) ),
            PlayPause => {
                controller_action!( coordinator.play_or_pause(): get_coordinator_for_name -> Ok(__) )
            }
            NextTrack => {
                controller_action!( coordinator.next(): get_coordinator_for_name -> Ok(__) )
            }
            PreviousTrack => {
                controller_action!( coordinator.previous(): get_coordinator_for_name -> Ok(__) )
            }
            SeekTime(seconds) => {
                data_action!( seconds.skip_to(coordinator: get_coordinator_for_name) -> Ok(__) )
            }
            SeekTrack(number) => {
                data_action!( number.seek_track(coordinator: get_coordinator_for_name) -> Ok(__) )
            }
            SeekRelTrack(number) => {
                data_action!( number.seek_rel_track(coordinatordata: get_coordinatordata_for_name) -> Ok(__) )
            }
            // TODO: SetRepeat and SetShuffle can be optimized to use cached info on playback state
            SetRepeat(mode) => {
                data_action!( mode.set(coordinator: get_coordinator_for_name) -> Ok(__) )
            }
            SetShuffle(state) => {
                data_action!( state.set_shuffle(coordinator: get_coordinator_for_name) -> Ok(__) )
            }
            SetCrossfade(state) => {
                data_action!( state.set_crossfade(coordinator: get_coordinator_for_name) -> Ok(__) )
            }
            SetPlayMode(mode, state) => {
                controller_action!( coordinator.set_playback_mode(mode, state): get_coordinator_for_name -> Ok(__) )
            }
            ClearQueue => {
                controller_action!( coordinator.clear_queue(): get_coordinator_for_name -> Ok(__) )
            }
            GetQueue => {
                controller_action!( coordinator.queue(): get_coordinator_for_name -> Queue(queue) )
            }
            ApplySnapshot(snapshot) => {
                controller_action!( coordinator.apply(snapshot): get_coordinator_for_name -> Ok(__) )
            }
            TakeSnapshot => {
                controller_action!( coordinator.snapshot(): get_coordinator_for_name -> Snapshot(snapshot) )
            }
            Exists => {
                if controller
                    .speakerdata
                    .iter()
                    .any(|s| s.speaker.name() == name)
                {
                    tx.send(Response::Ok(())).unwrap_or(());
                } else {
                    tx.send(Response::NotOk).unwrap_or(());
                }
            }
            SetRelVolume(number) => {
                data_action!( number.set_rel_volume(coordinator: get_coordinator_for_name) -> Ok(__) )
            }
        }

        Ok(())
    }
}

trait ZoneActionRepeatModeExt {
    async fn set(self, coordinator: &Speaker) -> Result<()>;
}

impl ZoneActionRepeatModeExt for RepeatMode {
    async fn set(self, speaker: &Speaker) -> Result<()> {
        speaker.set_repeat_mode(self).await.map_err(Error::from)
    }
}

trait ZoneActionBoolExt {
    async fn set_shuffle(self, speaker: &Speaker) -> Result<()>;
    async fn set_crossfade(self, speaker: &Speaker) -> Result<()>;
}

impl ZoneActionBoolExt for bool {
    async fn set_shuffle(self, speaker: &Speaker) -> Result<()> {
        speaker.set_shuffle(self).await.map_err(Error::from)
    }
    async fn set_crossfade(self, speaker: &Speaker) -> Result<()> {
        speaker.set_crossfade(self).await.map_err(Error::from)
    }
}

trait ZoneActionUnsignedNExt {
    async fn skip_to(self, speaker: &Speaker) -> Result<()>;
    async fn seek_track(self, speaker: &Speaker) -> Result<()>;
}

impl ZoneActionUnsignedNExt for u32 {
    async fn skip_to(self, speaker: &Speaker) -> Result<()> {
        speaker.skip_to(self).await.map_err(Error::from)
    }
    async fn seek_track(self, speaker: &Speaker) -> Result<()> {
        speaker.seek_track(self).await.map_err(Error::from)
    }
}

trait ZoneActionSignedNExt {
    async fn seek_rel_track(self, speakerdata: &SpeakerData) -> Result<()>;
    async fn set_rel_volume(self, speaker: &Speaker) -> Result<()>;
}

impl ZoneActionSignedNExt for i32 {
    async fn seek_rel_track(self, speakerdata: &SpeakerData) -> Result<()> {
        let cur_track_no: i32 = speakerdata
            .get_current_track_no()
            .await?
            .try_into()
            .or(Err(Error::ZoneActionError))?;
        let target = cur_track_no + self;
        if target < 1 {
            speakerdata.speaker.seek_track(1).await.map_err(Error::from)
        } else {
            log::debug!("Seeking to track: {}", target);
            speakerdata
                .speaker
                .seek_track(target as u32)
                .await
                .map_err(Error::from)
        }
    }

    async fn set_rel_volume(self, speaker: &Speaker) -> Result<()> {
        speaker
            .set_volume_relative(self)
            .await
            .map(|_| ())
            .map_err(Error::from)
    }
}
