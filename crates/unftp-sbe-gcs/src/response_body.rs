use super::ObjectMetadata;
use base64::Engine;
use chrono::prelude::*;
use libunftp::storage::{Error, ErrorKind, Fileinfo};
use serde::{Deserialize, de};
use std::fmt::{Display, Write};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::SystemTime;

#[derive(Deserialize, Debug)]
pub(crate) struct ResponseBody {
    items: Option<Vec<Item>>,
    prefixes: Option<Vec<String>>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
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
        let items: Vec<Fileinfo<PathBuf, ObjectMetadata>> = match &self.items {
            Some(items) => match &self.prefixes {
                Some(p) => items
                    .iter()
                    .filter(|item: &&Item| !item.name.ends_with('/') || p.contains(&item.name))
                    .map(|item: &Item| item.to_file_info().unwrap())
                    .collect(),
                None => items
                    .iter()
                    .filter(|item: &&Item| !item.name.ends_with('/'))
                    .map(|item: &Item| item.to_file_info().unwrap())
                    .collect(),
            },
            None => vec![],
        };

        // Files that weren't created through Google Console / Google Storage may exist in any prefix, and there may not be any 'prefix' object.
        // For instance, one could create an object 'subdir/subdir/file', and there won't be a 'subdir/' and 'subdir/subdir/' object.
        // So, we need to support those cases as well.
        // We don't have any metadata on these 'directories' though.
        let prefixes_without_object = self.prefixes.map_or(vec![], |prefixes: Vec<String>| {
            prefixes
                .iter()
                .filter(|prefix| self.items.as_ref().is_none_or(|it: &Vec<Item>| !it.iter().any(|i| i.name == **prefix)))
                .map(|prefix| Fileinfo {
                    path: prefix.into(),
                    metadata: ObjectMetadata {
                        last_updated: SystemTime::now(),
                        is_file: false,
                        size: 0,
                    },
                })
                .collect()
        });

        let result: &mut Vec<Fileinfo<PathBuf, ObjectMetadata>> = &mut vec![];
        result.extend(prefixes_without_object);
        result.extend(items);
        Ok(result.to_vec())
    }

    pub(crate) fn dir_exists(&self) -> bool {
        self.items.is_some() || self.prefixes.is_some()
    }

    pub(crate) fn dir_empty(&self) -> bool {
        // The directory is not empty if:
        // - nextPageToken is set (this indicates more than 1 entry, while we're using maxResults=2)
        // - prefixes is non empty (there are subdirs)
        // - there is more than 1 object within the prefix
        // - there is at least 1 object that is a file (does not end with /)
        match (self.next_page_token.as_ref(), self.prefixes.as_ref(), self.items.as_ref()) {
            (Some(_), _, _) => false,
            (_, Some(_), _) => false,
            (_, _, Some(items)) => items.len() == 1 && items[0].name.ends_with('/'),
            (_, _, _) => false,
        }
    }

    pub(crate) fn next_token(&self) -> Option<String> {
        self.next_page_token.as_ref().cloned()
    }
}

impl Item {
    pub(crate) fn to_metadata(&self) -> Result<ObjectMetadata, Error> {
        Ok(ObjectMetadata {
            size: self.size,
            last_updated: self.updated.into(),
            is_file: !self.name.ends_with('/'),
        })
    }

    pub(crate) fn to_file_info(&self) -> Result<Fileinfo<PathBuf, ObjectMetadata>, Error> {
        let path: PathBuf = PathBuf::from(self.name.clone());
        let metadata: ObjectMetadata = self.to_metadata()?;

        Ok(Fileinfo { path, metadata })
    }

    pub(crate) fn to_md5(&self) -> Result<String, Error> {
        let md5 = base64::engine::general_purpose::STANDARD
            .decode(&self.md5_hash)
            .map_err(|e| Error::new(ErrorKind::LocalError, e))?;
        Ok(md5.iter().fold(String::new(), |mut output, b| {
            let _ = write!(output, "{b:02x}");
            output
        }))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use libunftp::storage::Metadata;

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
        assert!(metadata.is_file);
    }

    #[test]
    fn to_metadata_parse_error() {
        let response: serde_json::error::Result<Item> = serde_json::from_str(r#"{"name":"", "updated":"2020-09-01T12:13:14Z", "size":8}"#);
        assert!(response.err().unwrap().is_data());
    }
}
