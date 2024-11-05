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

use controller::{Controller, SpeakerData};
use sonor::{Snapshot, Track};
use std::fmt::Write as _;
use tokio::sync::mpsc;
use tokio::{sync::oneshot, task::JoinHandle};
use types::{CmdSender, Response, ZoneActionResponder, ZoneName};
use types::{Result, StatusResponder};

use controller::zoneaction::ZoneAction;
pub use error::Error;
pub use mediasource::MediaSource;

#[derive(Debug)]
pub struct Manager {
    controller_handle: JoinHandle<()>,
    tx: CmdSender,
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
            .send(Command::DoZoneAction(tx, self.name.clone(), action))
            .await
            .map_err(|_| Error::ControllerOffline)?;
        rx.await.map_err(|_| Error::MessageRecvError)
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
    /// Try to create a new manager to control the first sonos system found on
    /// the network. If a system cannot be found, an error is returned.
    pub async fn try_new() -> Result<Manager> {
        Self::try_new_with_room(None).await
    }

    /// Try to create a new manager to control a sonos system that has a
    /// speaker with a certain room name. If the room name does not match any
    /// existing system, an error is returned.
    pub async fn try_new_with_room(room: Option<String>) -> Result<Manager> {
        let (tx, rx) = mpsc::channel(32);
        let mut controller = Controller::new(rx, room);
        controller.init().await?;
        log::debug!(
            "Initialized controller with devices:\n{}",
            controller
                .system
                .speakers()
                .iter()
                .fold(String::new(), |mut acc, device| {
                    let _ = writeln!(acc, "     - {}", device.name());
                    acc
                })
        );

        let controller_handle = tokio::spawn(async move { controller.run().await });

        Ok(Manager {
            controller_handle,
            tx,
        })
    }

    /// Get a zone by name. If the zone does not exist, an error is returned.
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
        self.controller_handle.abort();
    }
}

#[derive(Debug)]
pub enum Command {
    DoZoneAction(ZoneActionResponder, ZoneName, ZoneAction),
    GetStatus(StatusResponder),
    // Browse or search media
    // Subscribe to events
    // Management of controller?
}
