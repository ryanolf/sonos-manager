use crate::{Result};
use roxmltree::Node;

/// The content struct contains items from the content directory service
#[derive(Debug)]
pub struct Content {
    title: String,
    creator: Option<String>,
    album_art_uri: Option<String>,
    uri: Option<String>,
    metadata: Option<String>
}


impl Content {
    pub(crate) fn from_xml(node: Node<'_, '_>) -> Result<Self> {
        let mut title = None;
        let mut creator = None;
        let mut album_art_uri = None;
        let mut uri = None;
        let mut metadata = None;

        for child in node.children() {
            // log::debug!("{:?}", child.tag_name().name());
            match child.tag_name().name() {
                "title" => title = Some(child.text().unwrap_or_default().to_string()),
                "creator" => creator = Some(child.text().unwrap_or_default().to_string()),
                "albumArtURI" => album_art_uri = Some(child.text().unwrap_or_default().to_string()),
                "res" => uri = Some(child.text().unwrap_or_default().to_string()),
                "resMD" => metadata = Some(child.text().unwrap_or_default().to_string()),
                _ => (),
            }
        }

        let title = title.ok_or_else(|| {
            rupnp::Error::XmlMissingElement(node.tag_name().name().to_string(), "title".to_string())
        })?;

        Ok(Self {
            title,
            creator,
            album_art_uri,
            uri,
            metadata
        })
    }

    /// Get a reference to the content's title.
    pub fn title(&self) -> &str {
        self.title.as_str()
    }

    /// Get a reference to the content's creator.
    pub fn creator(&self) -> Option<&String> {
        self.creator.as_ref()
    }

    /// Get a reference to the content's album art uri.
    pub fn album_art_uri(&self) -> Option<&String> {
        self.album_art_uri.as_ref()
    }

    /// Get a reference to the content's uri.
    pub fn uri(&self) -> Option<&String> {
        self.uri.as_ref()
    }

    /// Get a reference to the content's metadata.
    pub fn metadata(&self) -> Option<&String> {
        self.metadata.as_ref()
    }
}
