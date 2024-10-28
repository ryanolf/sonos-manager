use roxmltree::{Document, Node};
use sonor::utils::find_root_node;

use super::Result;

pub fn extract_av_transport_last_change(state_xml: &str) -> Result<Vec<(String, String)>> {
    let doc = Document::parse(state_xml).map_err(sonor::Error::from)?;
    let state =
        find_root_node(&doc, "InstanceID", "Last Change Variables").map_err(sonor::Error::from)?;
    // let keys = ["CurrentPlayMode", "CurrentTrack", "CurrentCrossfadeMode", "AVTransportURI"];

    Ok(state
        .children()
        .filter(Node::is_element)
        // .filter(|c| keys.contains(&c.tag_name().name()))
        .map(|c| {
            (
                c.tag_name().name().to_string(),
                c.attribute("val").unwrap_or("").to_string(),
            )
        })
        .collect())
}
