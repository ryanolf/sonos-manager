#![allow(missing_docs)]

use futures_util::stream::StreamExt;

use log::{debug, error, info, warn};
use sonor::{extract_zone_topology, urns::AV_TRANSPORT};
use std::time::Duration;
use tokio::{self, sync::watch, task::JoinHandle, time};

use super::{
    types::{Event, EventReceiver, Uuid},
    utils::extract_av_transport_last_change,
    Error::SubscriberError,
    Result,
};

const TIMEOUT_SEC: u32 = 300;
const RENEW_SEC: u32 = 60;

type Sender = tokio::sync::watch::Sender<Event>;

/// Manages subscriptions to services. Returns a `tokio::sync::watch::Receiver`
/// that will carry the latest data. Will handle resubscribing as long as there
/// are still receiver handles. Once all receiver handles are dropped, the
/// subscriber will shutdown and will need to be recreated.
#[derive(Default, Debug)]
pub struct Subscriber {
    service: Option<sonor::rupnp::Service>,
    url: Option<sonor::rupnp::http::Uri>,
    pub uuid: Option<Uuid>,
    task_handle: Option<JoinHandle<Result<Sender>>>,
}

impl Subscriber {
    pub fn new() -> Subscriber {
        Subscriber::default()
    }

    pub fn subscribe(
        &mut self,
        service: sonor::rupnp::Service,
        url: sonor::rupnp::http::Uri,
        uuid: Option<Uuid>,
    ) -> Result<EventReceiver> {
        // Clone all so they can be moved later into async task
        self.service = Some(service);
        self.uuid = uuid;
        self.url = Some(url);

        if self.service.as_ref().unwrap().service_type() == AV_TRANSPORT && self.uuid.is_none() {
            return Err(SubscriberError(
                "Need UUID for AV_TRANSPORT Subscriptions!".into(),
            ));
        }

        // Create the notification channel
        let (tx, mut rx) = watch::channel(Event::NoOp);
        rx.borrow_and_update(); // Mark NoOp as read

        self.spawn_task(tx)?;
        Ok(rx)
    }

    /// Spawns the task that manages this subscription and listens for events.
    /// Returns the sender via the JoinHandle when all receivers are gone,
    /// allowing new receivers to potentially be created from it and this
    /// listening task re_spawned. The spawned task will return an error via
    /// join handle if the subscription cannot be made or maintained, e.g. the device
    /// goes offline. This function returns an error if service and url are not set.
    pub fn spawn_task(&mut self, tx: Sender) -> Result<()> {
        use Event::*;
        let service = self
            .service
            .as_ref()
            .ok_or_else(|| SubscriberError("No service defined!".to_string()))?
            .clone();
        let url = self
            .url
            .as_ref()
            .ok_or_else(|| SubscriberError("No url defined!".to_string()))?
            .clone();
        let uuid = self.uuid.clone();

        let task_handle = tokio::spawn(async move {
            let service_type = service.service_type();
            let (mut sid, mut stream) =
                service.subscribe(&url, TIMEOUT_SEC).await.map_err(|err| {
                    tx.send(SubscribeError(uuid.clone(), service_type.to_owned()))
                        .ok();
                    sonor::Error::UPnP(err)
                })?;
            let mut interval = time::interval(Duration::from_millis((RENEW_SEC * 1000).into()));
            loop {
                // Select over reading from the subscription stream, aborting
                // due to no more subscribers, and resubscription timer
                tokio::select! {
                    maybe_state_vars = &mut stream.next() => match maybe_state_vars {
                        Some(Ok(mut state_vars)) => match service_type.typ() {
                            "ZoneGroupTopology" => {
                                state_vars
                                    .remove("ZoneGroupState")
                                    .and_then(|xml| extract_zone_topology(&xml)
                                        .map_err(|err| warn!("Unable to extract topology: {}", err))
                                        .ok())
                                    .and_then(|topology| tx.send(TopoUpdate(uuid.clone(), topology)).ok());
                            }
                            "AVTransport" => {
                                state_vars
                                    .remove("LastChange")
                                    .and_then(|xml| extract_av_transport_last_change(&xml)
                                        .map_err(|err| warn!("Unable to extract last change: {}", err))
                                        .ok())
                                    .and_then(|last_change| tx.send(AVTransUpdate(uuid.clone(), last_change)).ok());
                            }
                            _ => ()

                        }
                        Some(Err(err)) => {
                            // There is an error from the TCP socket. Probably best to resubscribe.
                            warn!("TCP socket error: {}", err);
                            let new_sub = service.subscribe(&url, TIMEOUT_SEC)
                                .await
                                .map_err(|err| {
                                    tx.send(SubscribeError(uuid.clone(), service_type.to_owned())).ok();
                                    sonor::Error::UPnP(err)})?;
                            sid = new_sub.0;
                            stream = new_sub.1;
                        }
                        _ => {
                            error!("Subscription stream terminated?!?");
                            let new_sub = service.subscribe(&url, TIMEOUT_SEC)
                                .await
                                .map_err(|err| {
                                    tx.send(SubscribeError(uuid.clone(), service_type.to_owned())).ok();
                                    sonor::Error::UPnP(err)})?;
                            sid = new_sub.0;
                            stream = new_sub.1;
                        }
                    },
                    _ = tx.closed() => {
                        info!("No more subscribers. Shutting down {} on {}", service_type.typ(), uuid.as_deref().unwrap_or("unknown UUID"));
                        service.unsubscribe(&url, &sid).await.unwrap_or(());
                        break
                    },
                    _ = interval.tick() => {
                        let uuid = uuid.to_owned();
                        debug!("Attempting resubscribe to {} on {}...", service_type.typ(), uuid.as_deref().unwrap_or("unknown UUID"));
                        if let Err(err) = service.renew_subscription(&url, &sid, TIMEOUT_SEC).await {
                            info!("{} while resubscribing. Attempting new subscription", sonor::Error::UPnP(err));
                            let new_sub = service.subscribe(&url, TIMEOUT_SEC).await.map_err(|err| {
                                tx.send(SubscribeError(uuid, service_type.to_owned())).ok();
                                sonor::Error::UPnP(err)})?;
                            sid = new_sub.0;
                            stream = new_sub.1;
                        } else {
                            debug!("    ...{} on {} subscription renewed", service_type.typ(), uuid.as_deref().unwrap_or("unknown UUID"));
                        }
                    }
                }
            }
            Ok(tx)
        });

        self.task_handle = Some(task_handle);
        Ok(())
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        self.task_handle.as_ref().map(JoinHandle::abort);
    }
}
