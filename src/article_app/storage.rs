//! Robrix's Matrix publication metadata and shared article storage adapter.
use std::{path::Path, collections::BTreeMap, io::Read};
use serde::{Serialize, Deserialize};
use ruma::{OwnedRoomId, OwnedEventId};
use super::{document::*, model::Grant, host::RobrixArticleHost};
pub use article_core::assets::{Asset, MAX_FILE};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteAsset {
    pub asset: Asset,
    pub source: ruma::events::room::MediaSource,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Publication {
    pub id: String,
    pub document_id: String,
    pub room: OwnedRoomId,
    pub room_name: String,
    pub root: OwnedEventId,
    pub events: Vec<OwnedEventId>,
    pub version: u64,
    pub withdrawn: bool,
    pub modified: u64,
    pub document: Document,
    pub assets: BTreeMap<String, RemoteAsset>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationKind {
    Publish,
    Update,
    Withdraw,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub id: String,
    pub kind: OperationKind,
    pub document: Document,
    pub room: OwnedRoomId,
    pub room_name: String,
    pub publication: Option<String>,
    pub root: Option<OwnedEventId>,
    pub version: u64,
    pub uploaded: BTreeMap<String, RemoteAsset>,
    pub redacted: Vec<OwnedEventId>,
    pub confirmed: Option<OwnedEventId>,
    pub finished: bool,
}

pub type Library = article_core::storage::Library<Publication, Operation>;
type Store<'a> = article_core::storage::LocalStore<'a, RobrixArticleHost<'a>, Publication, Operation>;
pub fn load(root: &Path, grant: &Grant) -> Result<Library, String> {
    Store::new(&RobrixArticleHost::new(root), &grant.lease).load()
}
pub fn update<T>(root: &Path, grant: &Grant, edit: impl FnOnce(&mut Library) -> Result<T, String>) -> Result<T, String> {
    Store::new(&RobrixArticleHost::new(root), &grant.lease).update(edit)
}
pub fn save_document(root: &Path, grant: &Grant, document: &Document) -> Result<(), String> {
    Store::new(&RobrixArticleHost::new(root), &grant.lease).save_document(document)
}
pub fn asset_bytes(root: &Path, grant: &Grant, id: &str) -> Result<Vec<u8>, String> {
    Store::new(&RobrixArticleHost::new(root), &grant.lease).asset_bytes(id)
}
pub fn import_image(root: &Path, grant: &Grant, path: &Path, name: &str) -> Result<Asset, String> {
    use article_core::host::{ArticleHost, Capability};
    let host = RobrixArticleHost::new(root);
    host.authorize(&grant.lease, Capability::ImportAssets)?;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Store::new(&host, &grant.lease).import_image(&bytes, name)
}
