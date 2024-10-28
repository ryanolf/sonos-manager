use sonor::{SpeakerInfo, URN};
use tokio::sync::{mpsc, oneshot};

use crate::{Snapshot, Track};

use super::{Error, ZoneAction};

#[derive(Debug)]
pub(super) enum Command {
    DoZoneAction(Responder, ZoneName, ZoneAction),
    // Browse or search media
    // Subscribe to events
    // Management of controller?
}

#[derive(Debug)]
pub enum Response {
    Ok(()),
    NotOk,
    Snapshot(Snapshot),
    Queue(Vec<Track>),
}

#[derive(Debug, Clone)]
pub(super) enum Event {
    TopoUpdate(Option<Uuid>, Topology),
    AVTransUpdate(Option<Uuid>, AVStatus),
    SubscribeError(Option<Uuid>, URN),
    NoOp,
}

pub(crate) type Uuid = String;
pub(super) type CmdSender = mpsc::Sender<Command>;
pub(super) type EventReceiver = tokio::sync::watch::Receiver<Event>;

pub(super) type ReducedTopology = Vec<(Uuid, Vec<Uuid>)>;
pub(super) type Topology = Vec<(Uuid, Vec<SpeakerInfo>)>;
pub(super) type AVStatus = Vec<(String, String)>;
pub(super) type Result<T, E = Error> = std::result::Result<T, E>;

/// Type for zone name
pub type ZoneName = String;

/// Type for response channel
pub type Responder = oneshot::Sender<Response>;
