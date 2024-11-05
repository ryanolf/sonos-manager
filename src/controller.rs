#![allow(missing_docs)]

//! API backend for tracking sonos system topology

pub(crate) mod zoneaction;

use crate::{
    subscriber::Subscriber,
    types::{AVStatus, CmdReceiver, Event, EventReceiver, Topology, Uuid, ZoneActionResponder},
    Command, Error, Result,
};
use zoneaction::ZoneAction;

use futures_util::{stream::SelectAll, FutureExt as _};
use log::{debug, info, warn};
use sonor::{
    discover_one, find,
    urns::{AV_TRANSPORT, ZONE_GROUP_TOPOLOGY},
    Speaker,
};
use std::fmt::Write as _;
use std::time::Duration;
use tokio::select;
use tokio_stream::{wrappers::WatchStream, StreamExt as _};

#[derive(Debug)]
pub(crate) struct SpeakerData {
    pub speaker: Speaker,
    transport_subscription: Option<Subscriber>,
    pub transport_data: AVStatus,
}

impl SpeakerData {
    fn new(speaker: Speaker) -> SpeakerData {
        SpeakerData {
            speaker,
            transport_data: Default::default(),
            transport_subscription: Default::default(),
        }
    }

    /// Get the current track number for this speaker. Take value from cache if
    /// available, otherwise ask for it.
    pub async fn get_current_track_no(&self) -> Result<u32> {
        match self
            .transport_data
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("CurrentTrack"))
        {
            Some((_, track_no)) => {
                debug!("Using cached current track no: {}", track_no);
                track_no.parse().map_err(|_| Error::ContentNotFound)
            }
            None => self
                .speaker
                .track()
                .await
                .map(|o| o.map(|t| t.track_no()).unwrap_or(0))
                .map_err(Error::from),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct System {
    pub speakerdata: Vec<SpeakerData>,
    topology: Topology,
    queued_event_handles: Vec<EventReceiver>,
    topology_subscription: Option<Subscriber>,
    seed: Option<String>,
}

impl System {
    fn new(seed: Option<String>) -> System {
        System {
            seed,
            ..Default::default()
        }
    }

    async fn discover(&mut self) -> Result<()> {
        let topology = if let Some(ref room) = self.seed {
            debug!("Looking for seed: {}", room);
            find(room, Duration::from_secs(5))
                .await?
                .ok_or(Error::ZoneDoesNotExist)?
                .zone_group_state()
                .await?
        } else {
            discover_one(Duration::from_secs(5))
                .await?
                .zone_group_state()
                .await?
        };

        self.update_from_topology(topology).await?;
        self.update_topology_subscription()?;
        Ok(())
    }

    /// Get a reference to the vector of speakers.
    pub fn speakers(&self) -> Vec<&Speaker> {
        self.speakerdata.iter().map(|sd| &sd.speaker).collect()
    }

    /// Update speakers and topology
    async fn update_from_topology(&mut self, topology: Topology) -> Result<()> {
        let infos = topology.iter().flat_map(|(_, infos)| infos);

        // Drop speakers and subscriptions that are no longer in the topology
        // Todo: (speakers, av_transport_data, subscription) should probably be
        // a single tuple. Seems like we search them all together alot
        self.speakerdata.retain(|sd| {
            infos
                .clone()
                .any(|info| info.uuid().eq_ignore_ascii_case(sd.speaker.uuid()))
        });

        // Check if we have any new speakers in the system and add them. Update speaker info otherwise
        for info in infos {
            if let Some(speakerdata) = self
                .speakerdata
                .iter_mut()
                .find(|sd| sd.speaker.uuid().eq_ignore_ascii_case(info.uuid()))
            {
                speakerdata.speaker.set_name(info.name().into());
                speakerdata.speaker.set_location(info.location().into());
            } else {
                let new_speaker = Speaker::from_speaker_info(info)
                    .await?
                    .ok_or(sonor::Error::SpeakerNotIncludedInOwnZoneGroupState)?;

                // Subscribe to AV Transport events on new speakers
                let mut new_speakerdata = SpeakerData::new(new_speaker);
                if let Some((device_sub, rx)) =
                    get_av_transport_subscription(&new_speakerdata.speaker).await
                {
                    new_speakerdata.transport_subscription = Some(device_sub);
                    self.queued_event_handles.push(rx);
                }
                debug!("Adding UUID: {}", info.uuid());
                self.speakerdata.push(new_speakerdata);
            }
        }

        self.topology = topology;
        Ok(())
    }

    fn update_topology_subscription(&mut self) -> Result<()> {
        if self.speakerdata.is_empty() {
            return Err(sonor::Error::NoSpeakersDetected.into());
        }
        // Chose a random speaker. We may have lost subscription to topology
        // because the last speaker went offline.. and we don't know.
        // There's a chance we can recover quickly if we find an extant speaker.
        let i = fastrand::usize(..self.speakerdata.len());
        let device = self.speakerdata[i].speaker.device();
        let (service, url) = device
            .find_service(ZONE_GROUP_TOPOLOGY)
            .ok_or(sonor::Error::MissingServiceForUPnPAction {
                service: ZONE_GROUP_TOPOLOGY.clone(),
                action: String::new(),
                payload: String::new(),
            })
            .map(|service| (service.clone(), device.url().clone()))?;
        let mut sub = Subscriber::new(service, url, None);
        self.queued_event_handles.push(sub.subscribe()?);
        self.topology_subscription = Some(sub);
        Ok(())
    }
}

#[derive(Debug)]
/// The controller owns the Speakers and keeps track of the topology
/// so it can perform actions using the appropriate coordinating speakers.
/// It is constructed and then moved into a thread/task.
pub(crate) struct Controller {
    pub system: System,
    rx: CmdReceiver,
}

impl Controller {
    /// Make a new controller -- an "actor" that will handle maintenance of
    /// the sonos system state and dispatch commands to groups of speakers.
    ///
    /// If there are multiple sonos systems on the network, we can specify that
    /// we want the discovered system to have a speaker with certain name.
    /// Otherwise, the first speaker found will define the system and be used
    /// to build the system topology.
    pub fn new(rx: CmdReceiver, seed_room: Option<String>) -> Self {
        let system = System::new(seed_room);
        Controller { system, rx }
    }

    pub async fn init(&mut self) -> Result<()> {
        self.system.discover().await.map_err(|e| {
            info!("Unable to discover system: {}", e);
            e
        })?;
        self.system.update_topology_subscription().map_err(|e| {
            info!("Unable to get topology subscription: {}", e);
            e
        })
    }

    /// Handle events.
    async fn handle_event(&mut self, event: Event) {
        use Event::*;
        match event {
            TopoUpdate(_uuid, topology) => {
                debug!(
                    "Got topology update: {}",
                    topology.iter().fold(String::new(), |mut acc, (u, s)| {
                        let _ = write!(
                            acc,
                            "{} => {:?}, ",
                            self.get_speaker_by_uuid(u)
                                .map(|s| s.name())
                                .unwrap_or_default(),
                            s.iter().map(|i| i.name()).collect::<Vec<&str>>()
                        );
                        acc
                    })
                );
                self.system
                    .update_from_topology(topology)
                    .await
                    .unwrap_or_else(|err| warn!("Error updating system topology: {:?}", err))
            }
            AVTransUpdate(uuid, data) => {
                let keys = [
                    "CurrentPlayMode",
                    "CurrentTrack",
                    "TransportState",
                    "AVTransportURI",
                ];
                debug!(
                    "Got AVTransUpdate for {} (coord: {})",
                    self.get_speaker_by_uuid(uuid.as_ref().unwrap())
                        .map(|s| s.name())
                        .unwrap_or_default(),
                    self.get_coordinator_for_uuid(uuid.as_ref().unwrap())
                        .map(|s| s.name())
                        .unwrap_or_default()
                );
                debug!(
                    "... {:?}",
                    data.iter()
                        .filter(|(s, _)| keys.contains(&s.as_str()))
                        .collect::<Vec<&(String, String)>>()
                );
                if let Some(uuid) = uuid {
                    self.update_avtransport_data(uuid, data)
                } else {
                    warn!("Missing UUID for AV Transport update")
                }
            }
            SubscribeError(uuid, urn) => {
                debug!(
                    "Subscription {} on {} lost",
                    urn,
                    uuid.as_deref().unwrap_or("unknown")
                );
                // I'd like to just match the URN to the defined constants, but
                // that leads to "Indirect Structural Match" lint error
                match urn.typ() {
                    "ZoneGroupTopology" => {
                        // The speaker we were getting updates from may have gone offline. Try another
                        if let Err(err) = self.system.update_topology_subscription() {
                            info!("Having trouble subscribing to topology updates: {}", err);
                            info!("  ...attempting to rediscover system");
                            match self.system.discover().await {
                                Ok(_) => info!("  ...success!"),
                                Err(err) => {
                                    info!("  ...failed: {}", err);
                                    self.system.topology_subscription.take();
                                }
                            }
                        }
                    }
                    "AVTransport" => {
                        // The speaker we are subscribing to may have gone
                        // offline or gotten a new IP. In case its the later,
                        // the SpeakerInfo and Device could be out of sync
                        let uuid = &uuid.unwrap();
                        if let Some(speakerdata) = self.get_mut_speakerdata_by_uuid(uuid) {
                            if let Ok(Some(speaker)) =
                                Speaker::from_speaker_info(speakerdata.speaker.info()).await
                            {
                                // The speaker still exists! Resubscribe
                                debug!(
                                    "Recreating speaker {}. Did it's IP change?",
                                    speaker.name()
                                );
                                match get_av_transport_subscription(&speaker).await {
                                    Some((sub, rx)) => {
                                        speakerdata.transport_subscription = Some(sub);
                                        self.system.queued_event_handles.push(rx);
                                    }
                                    None => speakerdata.transport_subscription = None,
                                }
                            }
                        }
                    }
                    _ => (),
                }
            }
            NoOp => (),
        };
    }

    /// Handle zone actions. Deal with errors here. Only return an error if it
    /// is unrecoverable and should break the non-event loop.
    async fn handle_zone_action(&self, tx: ZoneActionResponder, name: String, action: ZoneAction) {
        debug!("Handling action {:?} for zone {}", action, name);
        action.handle_action(self, tx, name).await
    }

    /// Run the event loop.
    ///
    /// - Subscribe and listen to events on the sonos system, maintaining
    ///   the system state up-to-date.
    /// - Rediscover system as needed
    /// - Listen for commands from clients to perform actions on zones.

    pub async fn run(&mut self) {
        use Command::*;

        let mut event_stream = SelectAll::new();

        debug!("Listening for commands");
        'outer: loop {
            event_stream.extend(
                self.system
                    .queued_event_handles
                    .drain(..)
                    .map(WatchStream::new),
            );
            if self.system.topology_subscription.is_none() {
                let now = tokio::time::Instant::now();
                info!("Lost system. Rediscovering...");
                match self.init().await {
                    Ok(_) => info!("  ...success!"),
                    Err(err) => {
                        info!("  ...failed: {}", err);
                        // Handle any pending commands without awaiting
                        'inner: loop {
                            match self.rx.try_recv() {
                                Ok(Command::DoZoneAction(tx, name, action)) => {
                                    self.handle_zone_action(tx, name, action).await;
                                }
                                Ok(Command::GetStatus(_sender)) => {
                                    todo!()
                                }
                                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break 'inner,
                                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                                    break 'outer
                                }
                            }
                        }

                        // Handle any pending events without awaiting
                        while let Some(Some(event)) = event_stream.next().now_or_never() {
                            self.handle_event(event).await;
                        }
                        tokio::time::sleep(Duration::from_secs(1).saturating_sub(now.elapsed()))
                            .await;
                        continue 'outer;
                    }
                }
            }
            select! {
                maybe_command = self.rx.recv() => match maybe_command {
                    Some(cmd) => match cmd {
                        DoZoneAction(tx,name,action)=>self.handle_zone_action(tx,name,action).await,
                        GetStatus(_sender) => todo!(), },
                    None => break
                },
                maybe_event = event_stream.next() => match maybe_event {
                    Some(event) => self.handle_event(event).await,
                    None => info!("No active subscriptions... all devices unreachable?"),
                }
            }
        }
        debug!("Controller loop finished");
    }

    fn get_speaker_with_name(&self, name: &str) -> Option<&Speaker> {
        self.system.speakerdata.iter().find_map(|s| {
            match s.speaker.name().eq_ignore_ascii_case(name) {
                true => Some(&s.speaker),
                false => None,
            }
        })
    }

    fn get_speaker_by_uuid(&self, uuid: &str) -> Option<&Speaker> {
        self.system.speakerdata.iter().find_map(|s| {
            match s.speaker.uuid().eq_ignore_ascii_case(uuid) {
                true => Some(&s.speaker),
                false => None,
            }
        })
    }

    fn get_speakerdata_by_uuid(&self, uuid: &str) -> Option<&SpeakerData> {
        self.system
            .speakerdata
            .iter()
            .find(|s| s.speaker.uuid().eq_ignore_ascii_case(uuid))
    }

    fn get_mut_speakerdata_by_uuid(&mut self, uuid: &str) -> Option<&mut SpeakerData> {
        self.system
            .speakerdata
            .iter_mut()
            .find(|s| s.speaker.uuid().eq_ignore_ascii_case(uuid))
    }

    pub fn get_coordinator_for_name(&self, name: &str) -> Option<&Speaker> {
        let speaker = self.get_speaker_with_name(name)?;
        self.get_coordinator_for_uuid(speaker.uuid())
    }

    pub fn get_coordinatordata_for_name(&self, name: &str) -> Option<&SpeakerData> {
        let speaker = self.get_speaker_with_name(name)?;
        self.get_coordinatordata_for_uuid(speaker.uuid())
    }

    fn get_coordinator_for_uuid(&self, speaker_uuid: &str) -> Option<&Speaker> {
        let coordinator_uuid =
            self.system
                .topology
                .iter()
                .find_map(|(coordinator_uuid, uuids)| {
                    uuids
                        .iter()
                        .find(|&info| info.uuid().eq_ignore_ascii_case(speaker_uuid))
                        .and(Some(coordinator_uuid))
                })?;
        self.get_speaker_by_uuid(coordinator_uuid)
    }

    fn get_coordinatordata_for_uuid(&self, speaker_uuid: &str) -> Option<&SpeakerData> {
        let coordinator_uuid =
            self.system
                .topology
                .iter()
                .find_map(|(coordinator_uuid, uuids)| {
                    uuids
                        .iter()
                        .find(|&info| info.uuid().eq_ignore_ascii_case(speaker_uuid))
                        .and(Some(coordinator_uuid))
                })?;
        self.get_speakerdata_by_uuid(coordinator_uuid)
    }

    fn update_avtransport_data(&mut self, uuid: Uuid, data: Vec<(String, String)>) {
        match self
            .system
            .speakerdata
            .iter_mut()
            .find(|sd| sd.speaker.uuid().eq_ignore_ascii_case(&uuid))
        {
            Some(sd) => sd.transport_data = data,
            None => warn!(
                "Received AV Transport data for non-existant speaker {}",
                uuid
            ),
        };
    }

    /// Drop a speaker for no good reason
    #[cfg(test)]
    pub fn _drop_speaker(&mut self) {
        self.system.speakerdata.pop().unwrap();
    }
}

async fn get_av_transport_subscription(
    new_speaker: &Speaker,
) -> Option<(Subscriber, EventReceiver)> {
    if let Some(service) = new_speaker.device().find_service(AV_TRANSPORT) {
        let mut device_sub = Subscriber::new(
            service.clone(),
            new_speaker.device().url().clone(),
            Some(new_speaker.uuid().to_owned()),
        );
        if let Ok(rx) = device_sub.subscribe() {
            return Some((device_sub, rx));
        }
    }
    None
}

#[cfg(test)]
mod test {
    use tokio::sync::mpsc;

    #[allow(unused_imports)]
    use super::*;

    #[tokio::test]
    async fn test_controller() -> Result<()> {
        simple_logger::init_with_level(log::Level::Debug).unwrap();
        let handle = {
            let (_tx, rx) = mpsc::channel(10);
            let mut controller = Controller::new(rx, None);
            controller.init().await?;

            log::info!("Initialized manager with devices:");
            for device in controller.system.speakers().iter() {
                log::info!("     - {}", device.name());
            }

            controller._drop_speaker();

            log::info!("Now we have:");
            for device in controller.system.speakers().iter() {
                log::info!("     - {}", device.name());
            }

            let handle = tokio::spawn(async move {
                controller.run().await;
            });

            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            handle
        }; // drop _tx
        handle.await.unwrap();
        Ok(())
    }
}
