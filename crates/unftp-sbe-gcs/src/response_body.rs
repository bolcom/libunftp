use super::ObjectMetadata;
use chrono::prelude::*;
use libunftp::storage::{Error, ErrorKind, Fileinfo};
use serde::{de, Deserialize};
use std::fmt::Display;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Deserialize, Debug)]
pub(crate) struct ResponseBody {
    items: Option<Vec<Item>>,
    prefixes: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct Item {
    name: String,
    updated: DateTime<Utc>,

    // GCS API defines `size` as json string, doh
    #[serde(default, deserialize_with = "item_size_deserializer")]
    size: u64,
    #[serde(default, rename = "md5Hash")]
    md5_hash: String,
}

// TODO: this is a generic string->* deserializer, move to a util package
fn item_size_deserializer<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: FromStr,
    T::Err: Display,
    D: de::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse().map_err(de::Error::custom)
}

impl ResponseBody {
    pub(crate) fn list(self) -> Result<Vec<Fileinfo<PathBuf, ObjectMetadata>>, Error> {
        // The GCS API returns items[] (objects) including the 'directories' (because of includeTrailingDelimiter=true; see API)
        // We want this, because we need the metadata of these directories too (e.g. timestamp)
        // But the side behavior is that _the prefix itself_ is also included in items[] (yet, not in prefixes[]),
        // We don't want to return the prefix to the client, so we need to filter this one out
        // (E.g.: otherwise listing /level1/ would include 'level1/' also in its listing)
        // We filter this by returning only prefixes that exist in prefixes[]
        // See https://cloud.google.com/storage/docs/json_api/v1/objects/list
        self.items.map_or(Ok(vec![]), move |items: Vec<Item>| match self.prefixes {
            Some(p) => items
                .iter()
                .filter(|item: &&Item| !item.name.ends_with('/') || p.contains(&item.name))
                .map(move |item: &Item| item.to_file_info())
                .collect(),
            None => items
                .iter()
                .filter(|item: &&Item| !item.name.ends_with('/'))
                .map(move |item: &Item| item.to_file_info())
                .collect(),
        })
    }
}

impl Item {
    pub(crate) fn to_metadata(&self) -> Result<ObjectMetadata, Error> {
        Ok(ObjectMetadata {
            size: self.size,
            last_updated: Some(self.updated.into()),
            is_file: !self.name.ends_with('/'),
        })
    }

    pub(crate) fn to_file_info(&self) -> Result<Fileinfo<PathBuf, ObjectMetadata>, Error> {
        let path: PathBuf = PathBuf::from(self.name.clone());
        let metadata: ObjectMetadata = self.to_metadata()?;

        Ok(Fileinfo { path, metadata })
    }

    pub(crate) fn to_md5(&self) -> Result<String, Error> {
        let md5 = base64::decode(&self.md5_hash).map_err(|e| Error::new(ErrorKind::LocalError, e))?;
        Ok(md5.iter().map(|b| format!("{:02x}", b)).collect())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use libunftp::storage::Metadata;
    use std::time::SystemTime;

    #[test]
    fn to_metadata() {
        let sys_time = SystemTime::now();
        let date_time = DateTime::from(sys_time);

        let item: Item = Item {
            name: "".into(),
            updated: date_time,
            size: 50,
            md5_hash: "".into(),
        };

        let metadata: ObjectMetadata = item.to_metadata().unwrap();
        assert_eq!(metadata.size, 50);
        assert_eq!(metadata.modified().unwrap(), sys_time);
        assert_eq!(metadata.is_file, true);
    }

    #[test]
    fn to_metadata_parse_error() {
        let response: serde_json::error::Result<Item> = serde_json::from_str(r#"{"name":"", "updated":"2020-09-01T12:13:14Z", "size":8}"#);
        assert_eq!(response.err().unwrap().is_data(), true);
    }
}
