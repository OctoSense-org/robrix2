//! Account-scoped, atomic storage for drafts, immutable assets and the outbox.
use std::{
    path::{Path, PathBuf},
    io::{Read, Write},
    collections::BTreeMap,
    sync::Mutex,
};
use serde::{Serialize, Deserialize};
use ruma::{OwnedRoomId, OwnedEventId};
use super::{document::*, model::Grant};
static STORE: Mutex<()> = Mutex::new(());
pub const MAX_FILE: u64 = 12 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub mime: String,
    pub bytes: u64,
}
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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Library {
    pub schema: u32,
    pub documents: Vec<Document>,
    pub assets: BTreeMap<String, Asset>,
    pub publications: Vec<Publication>,
    pub outbox: Vec<Operation>,
    #[serde(default)]
    pub legacy_source: Option<String>,
}
impl Default for Library {
    fn default() -> Self {
        Self {
            schema: 2,
            documents: Vec::new(),
            assets: BTreeMap::new(),
            publications: Vec::new(),
            outbox: Vec::new(),
            legacy_source: None,
        }
    }
}
pub fn directory(root: &Path, grant: &Grant) -> PathBuf {
    root.join("mini-apps")
        .join(blake3::hash(grant.owner.as_bytes()).to_hex().as_str())
        .join("org.octosense.article-editor")
}
fn check(grant: &Grant) -> Result<(), String> {
    if !grant.valid(crate::sliding_sync::current_user_id().as_deref()) {
        Err("Authorization expired".into())
    } else {
        Ok(())
    }
}
fn load_inner(root: &Path, grant: &Grant) -> Result<Library, String> {
    let path = directory(root, grant).join("library-v2.json");
    match std::fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(8_000_001)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > 8_000_000 {
                return Err("Article library exceeds its storage limit.".into());
            }
            let lib: Library = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if lib.schema != 2 || lib.documents.len() > 100 {
                return Err("Unsupported article library".into());
            }
            for doc in &lib.documents {
                doc.validate()?
            }
            Ok(lib)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut lib = Library::default();
            let old = super::model::read_draft(root, grant, Some(&grant.owner))?;
            if !old.title.is_empty() || !old.markdown.is_empty() {
                match Document::from_markdown(&old.title, &old.markdown) {
                    Ok(d) => lib.documents.push(d),
                    Err(_) => {
                        lib.documents.push(Document {
                            title: old.title,
                            ..Document::default()
                        });
                        lib.legacy_source = Some(old.markdown);
                    }
                }
            }
            Ok(lib)
        }
        Err(e) => Err(e.to_string()),
    }
}
pub fn load(root: &Path, grant: &Grant) -> Result<Library, String> {
    check(grant)?;
    let _lock = STORE.lock().map_err(|_| "Article storage unavailable")?;
    load_inner(root, grant)
}
pub fn update<T>(
    root: &Path,
    grant: &Grant,
    edit: impl FnOnce(&mut Library) -> Result<T, String>,
) -> Result<T, String> {
    check(grant)?;
    let _lock = STORE.lock().map_err(|_| "Article storage unavailable")?;
    let mut lib = load_inner(root, grant)?;
    let value = edit(&mut lib)?;
    if lib.documents.len() > 100 {
        return Err("Keep at most 100 article drafts on this device.".into());
    }
    for doc in &lib.documents {
        doc.validate()?
    }
    let bytes = serde_json::to_vec(&lib).map_err(|e| e.to_string())?;
    if bytes.len() > 8_000_000 {
        return Err("Article library exceeds its storage limit.".into());
    }
    check(grant)?;
    atomic_write(&directory(root, grant).join("library-v2.json"), &bytes)?;
    Ok(value)
}
pub fn save_document(root: &Path, grant: &Grant, doc: &Document) -> Result<(), String> {
    doc.validate()?;
    update(root, grant, |lib| {
        if let Some(old) = lib.documents.iter_mut().find(|d| d.id == doc.id) {
            *old = doc.clone()
        } else {
            lib.documents.push(doc.clone())
        }
        Ok(())
    })
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("Invalid storage path")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temporary = path.with_extension(format!("{}.tmp", new_id()));
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temporary).map_err(|e| e.to_string())?;
        file.write_all(bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, path).map_err(|e| e.to_string())
    })(); // Both draft and outbox use the same atomic replacement contract.
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}
pub fn asset_path(root: &Path, grant: &Grant, id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err("Invalid image reference".into());
    }
    Ok(directory(root, grant).join("assets").join(id))
}
pub fn asset_bytes(root: &Path, grant: &Grant, id: &str) -> Result<Vec<u8>, String> {
    check(grant)?;
    let file = std::fs::File::open(asset_path(root, grant, id)?)
        .map_err(|_| "An article image is missing. Replace it before publishing.")?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_FILE || blake3::hash(&bytes).to_hex().as_str() != id {
        return Err("Article image failed its integrity check.".into());
    }
    Ok(bytes)
}
pub fn import_image(root: &Path, grant: &Grant, path: &Path, name: &str) -> Result<Asset, String> {
    check(grant)?;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let (asset, output) = prepare_image(&bytes, name)?;
    let id = asset.id.clone();
    check(grant)?;
    atomic_write(&asset_path(root, grant, &id)?, &output)?;
    update(root, grant, |lib| {
        if lib.assets.len() >= 300 && !lib.assets.contains_key(&id) {
            return Err("The image library is full.".into());
        }
        lib.assets.insert(id, asset.clone());
        Ok(())
    })?;
    Ok(asset)
}

fn prepare_image(bytes: &[u8], name: &str) -> Result<(Asset, Vec<u8>), String> {
    if bytes.len() as u64 > MAX_FILE {
        return Err("Choose a JPEG or PNG image up to 12 MB.".into());
    }
    let format =
        image::guess_format(&bytes).map_err(|_| "Choose a JPEG or PNG image up to 12 MB.")?;
    if !matches!(format, image::ImageFormat::Jpeg | image::ImageFormat::Png) {
        return Err("Choose a JPEG or PNG image up to 12 MB.".into());
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(128 * 1024 * 1024);
    limits.max_image_width = Some(12000);
    limits.max_image_height = Some(12000);
    reader.limits(limits);
    use image::ImageDecoder;
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let (w, h) = decoder.dimensions();
    if w == 0 || h == 0 || (w as u64 * h as u64) > 24_000_000 {
        return Err("Choose an image with at most 24 megapixels.".into());
    }
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut decoded = image::DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    decoded.apply_orientation(orientation);
    let resized = if decoded.width() > 2048 || decoded.height() > 2048 {
        decoded.resize(2048, 2048, image::imageops::FilterType::Lanczos3)
    } else {
        decoded
    };
    let mut output = std::io::Cursor::new(Vec::new());
    resized
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let output = output.into_inner();
    if output.len() as u64 > MAX_FILE {
        return Err("This image is too large after processing.".into());
    }
    let id = blake3::hash(&output).to_hex().to_string();
    let asset = Asset {
        id: id.clone(),
        name: name.chars().filter(|c| !c.is_control()).take(160).collect(),
        width: resized.width(),
        height: resized.height(),
        mime: "image/png".into(),
        bytes: output.len() as u64,
    };

    Ok((asset, output))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_import_normalizes_and_bounds_decoding() {
        let original = include_bytes!("../../lab/article-editor-v2/artwork/mountains.png");
        let (asset, bytes) = prepare_image(original, "山谷.png").unwrap();
        assert_eq!(asset.id, blake3::hash(&bytes).to_hex().as_str());
        assert_eq!(asset.width, 490);
        assert_eq!(asset.height, 233);
        assert_eq!(asset.mime, "image/png");
        assert!(prepare_image(b"<svg><script/></svg>", "bad.svg").is_err());
        assert!(prepare_image(&vec![0u8; MAX_FILE as usize + 1], "large.png").is_err());
    }
    #[test]
    fn atomic_replace_preserves_bytes_and_private_permissions() {
        let dir = std::env::temp_dir().join(new_id());
        let path = dir.join("library.json");
        atomic_write(&path, b"first").unwrap();
        atomic_write(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
