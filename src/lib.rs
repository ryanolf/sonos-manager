#![allow(missing_docs, unused_macros)]

//! A user-friendly API for controlling sonos systems similar to the
//! controller app, with room-by-room (or group-by-group) controls.

mod controller;
mod error;
mod mediasource;
mod metadata;
mod subscriber;
mod types;
pub mod utils;

#[cfg(test)]
mod test;

use controller::{Controller, SpeakerData};
use sonor::{Snapshot, Track};
use types::{CmdSender, Command, Response};

pub(self) use controller::ZoneAction;
pub use error::Error;
pub use mediasource::MediaSource;
pub(self) use types::Result;

use tokio::{sync::oneshot, task::JoinHandle};

#[derive(Default, Debug)]
pub struct Manager {
    controller_handle: Option<JoinHandle<Controller>>,
    tx: Option<CmdSender>,
}

#[derive(Debug)]
pub struct Zone<'a> {
    manager: &'a Manager,
    name: String,
}

macro_rules! action {
    ($fn:ident: $action:ident$(($($invar:ident: $intyp:ty),+))? => $resp:ident($outvar:ident: $outtyp:ty)) => {
        pub async fn $fn(&self$($(, $invar: $intyp)+)?)-> Result<$outtyp>{
            use ZoneAction::*;
            match self.action($action$(($($invar),+))?).await? {
                Response::$resp($outvar) => Ok($outvar),
                _ => Err(Error::ZoneActionError)
            }
        }
    };
}

impl<'a> Zone<'a> {
    pub async fn action(&self, action: ZoneAction) -> Result<Response> {
        let (tx, rx) = oneshot::channel();
        self.manager
            .tx
            .as_ref()
            .ok_or(Error::ControllerNotInitialized)?
            .send(Command::DoZoneAction(tx, self.name.clone(), action))
            .await
            .map_err(|_| Error::ControllerOffline)?;
        rx.await.map_err(|_| Error::MessageRecvError)
    }

    pub async fn update_room(&mut self, room_name: String) -> Result<()> {
        self.name = self.manager.get_zone(room_name).await?.name;
        Ok(())
    }

    action!(play_now: PlayNow(media: MediaSource) => Ok(__: ()));
    action!(queue_as_next: QueueAsNext(media: MediaSource) => Ok(__: ()));
    action!(play: Play => Ok(__: ()));
    action!(pause: Pause => Ok(__: ()));
    action!(play_or_pause: PlayPause => Ok(__: ()));
    action!(next_track: NextTrack => Ok(__: ()));
    action!(previous_track: PreviousTrack => Ok(__: ()));
    action!(seek_time: SeekTime(seconds: u32) => Ok(__: ()));
    action!(seek_track: SeekTrack(number: u32) => Ok(__: ()));
    action!(seek_rel_track: SeekRelTrack(number: i32) => Ok(__: ()));
    action!(set_repeat: SetRepeat(mode: sonor::RepeatMode) => Ok(__: ()));
    action!(set_shuffle: SetShuffle(state: bool) => Ok(__: ()));
    action!(set_crossfade: SetCrossfade(state: bool) => Ok(__: ()));
    action!(set_play_mode: SetPlayMode(mode: sonor::RepeatMode, state: bool) => Ok(__: ()));
    action!(clear_queue: ClearQueue => Ok(__: ()));
    action!(get_queue: GetQueue => Queue(queue: Vec<Track>));
    action!(take_snapshot: TakeSnapshot => Snapshot(snap: Snapshot));
    action!(apply_snapshot: ApplySnapshot(snap: Snapshot) => Ok(__: ()));
    action!(set_rel_volume: SetRelVolume(number: i32) => Ok(__: ()));
}

impl Manager {
    pub async fn new() -> Result<Manager> {
        let controller = Controller::new();
        Self::new_with_controller(controller).await
    }

    pub async fn new_with_roomname(room: &str) -> Result<Manager> {
        let mut controller = Controller::new();
        controller.seed_by_roomname(room).await?;
        Self::new_with_controller(controller).await
    }

    async fn new_with_controller(mut controller: Controller) -> Result<Manager> {
        let tx = Some(controller.init().await?);
        log::debug!("Initialized controller with devices:");
        for device in controller.speakers().iter() {
            log::debug!("     - {}", device.name());
        }

        let controller_handle = Some(tokio::spawn(async move {
            if let Err(e) = controller.run().await {
                log::error!("Controller shut down: {}", e)
            };
            log::debug!("Controller terminated on purpose?");
            controller
        }));

        Ok(Manager {
            controller_handle,
            tx,
        })
    }

    pub async fn get_zone(&self, room_name: String) -> Result<Zone<'_>> {
        let zone = Zone {
            manager: self,
            name: room_name,
        };
        match zone.action(ZoneAction::Exists).await? {
            Response::Ok(_) => Ok(zone),
            _ => Err(Error::ZoneDoesNotExist),
        }
    }
}

impl Drop for Manager {
    // The controller should shut down when we drop the transmitter, but just in case.
    fn drop(&mut self) {
        log::debug!("Dropping manager",);
        self.controller_handle.as_ref().map(JoinHandle::abort);
    }
}
