//! Explicit profile isolation for local UI regression tests.

use std::path::{Path, PathBuf};

pub(crate) fn resolve_app_data_dir(override_dir: Option<&Path>, default: &Path) -> Result<PathBuf, String> {
    match override_dir {
        None => Ok(default.to_path_buf()),
        Some(path) if path.is_absolute() => Ok(path.to_path_buf()),
        Some(_) => Err("ROBRIX_DATA_DIR must be a non-empty absolute path".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn data_directory_override_is_explicit() {
        let base = std::env::temp_dir();
        let default = base.join("robrix-default");
        let isolated = base.join("robrix-isolated");
        assert_eq!(resolve_app_data_dir(None, &default).unwrap(), default);
        assert_eq!(resolve_app_data_dir(Some(isolated.as_path()), &default).unwrap(), isolated);
        assert!(resolve_app_data_dir(Some(Path::new("relative")), &default).is_err());
        assert!(resolve_app_data_dir(Some(Path::new("")), &default).is_err());
    }

    proptest::proptest! {
        #[test]
        fn prop_data_directory_never_implicitly_changes_default(name in "[a-zA-Z0-9_-]{1,32}") {
            let base = std::env::temp_dir().join("robrix-default");
            let isolated = std::env::temp_dir().join(name);
            proptest::prop_assert_eq!(resolve_app_data_dir(None, &base).unwrap(), base.clone());
            proptest::prop_assert_eq!(resolve_app_data_dir(Some(&isolated), &base).unwrap(), isolated);
        }
    }
}
