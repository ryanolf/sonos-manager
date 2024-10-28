use sonor::{SpeakerInfo, URN};
use tokio::sync::{mpsc, oneshot};

use crate::{Command, Snapshot, Track};

use super::Error;

#[derive(Debug)]
pub enum Response {
    Ok(()),
    NotOk,
    Snapshot(Snapshot),
    Queue(Vec<Track>),
}

#[derive(Debug, Clone)]
pub enum Event {
    TopoUpdate(Option<Uuid>, Topology),
    AVTransUpdate(Option<Uuid>, AVStatus),
    SubscribeError(Option<Uuid>, URN),
    NoOp,
}

pub type Uuid = String;
pub type CmdSender = mpsc::Sender<Command>;
pub type EventReceiver = tokio::sync::watch::Receiver<Event>;

pub type ReducedTopology = Vec<(Uuid, Vec<Uuid>)>;
pub type Topology = Vec<(Uuid, Vec<SpeakerInfo>)>;
pub type AVStatus = Vec<(String, String)>;
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Type for zone name
pub type ZoneName = String;

/// Type for response channel
pub type Responder = oneshot::Sender<Response>;
