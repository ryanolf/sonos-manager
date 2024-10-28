use sonor::utils::escape_str_pcdata;
use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), sonor::Error> {
    let transport_uri = "x-sonos-http:librarytrack:a.1442979904.mp4?sid=204";
    // let transport_uri = "x-rincon-cpcontainer:1006206cplaylist:pl.cf589c8b40dc40cd9ddc2e61493d5efd?sid=204";
    let speaker = sonor::find("Living Room", Duration::from_secs(3))
        .await?
        .unwrap();

    let snapshot = speaker.snapshot().await?;
    println!("{:#?}", snapshot);

    speaker.set_volume(10).await?;
    let metadata = r#"
<DIDL-Lite xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/" xmlns:r="urn:schemas-rinconnetworks-com:metadata-1-0/" xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/">
    <item id="10032020librarytrack%3aa.1442979904" restricted="true" parentID="1004206calbum%3a">
        <dc:title></dc:title>
        <upnp:class>object.item.audioItem.musicTrack</upnp:class>
        <desc id="cdudn" nameSpace="urn:schemas-rinconnetworks-com:metadata-1-0/">SA_RINCON52231_X_#Svc52231-0-Token</desc>
    </item>
</DIDL-Lite>"#;
    speaker
        .queue_next(transport_uri, &escape_str_pcdata(metadata), Some(0))
        .await?;
    println!("{:#?}", speaker.queue().await?[3]);
    speaker.play().await?;
    tokio::time::sleep(Duration::from_secs(10)).await;

    speaker.apply(snapshot).await?;

    Ok(())
}
