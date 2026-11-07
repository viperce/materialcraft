use std::io::{Cursor, Read};

use materialcraft_core::{Package, ResourceBytes};

use crate::{FormatError, MAX_JSON, MAX_RESOURCE, Result, safe_file_name};

/// The older native package extension
pub const LEGACY_PACKAGE_EXT: &str = "mcraft";
const PACKAGE_JSON: &str = "package.json";
const RESOURCES_PREFIX: &str = "resources/";
/// Most resource records a legacy package may list.
const MAX_RESOURCES: usize = 4096;
/// Most decompressed bytes a legacy package may hold across `package.json` and all its resources.
const MAX_PACKAGE_BYTES: u64 = 2 << 30;

/// Whether `bytes` look like a legacy `.mcraft` package (a zip holding `package.json`)
pub fn is_legacy_package(bytes: &[u8]) -> bool {
    zip::ZipArchive::new(Cursor::new(bytes)).is_ok_and(|mut z| z.by_name(PACKAGE_JSON).is_ok())
}

/// Read a legacy `.mcraft` package into the current model. The document saves as native SBS.
pub fn read_legacy_package(bytes: &[u8]) -> Result<Package> {
    let bad = |m: String| FormatError::BadPackage(format!("legacy .mcraft package: {m}"));
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| bad(e.to_string()))?;
    let mut json = Vec::new();
    z.by_name(PACKAGE_JSON)
        .map_err(|_| bad(format!("{PACKAGE_JSON} is missing")))?
        .take(MAX_JSON + 1)
        .read_to_end(&mut json)
        .map_err(|e| FormatError::Io(e.to_string()))?;
    if json.len() as u64 > MAX_JSON {
        return Err(bad(format!("{PACKAGE_JSON} exceeds {MAX_JSON} bytes")));
    }
    let mut pkg: Package = serde_json::from_slice(&json).map_err(|e| bad(e.to_string()))?;
    if pkg.resources.len() > MAX_RESOURCES {
        return Err(bad(format!("{} resources (limit {MAX_RESOURCES})", pkg.resources.len())));
    }
    let mut total = json.len() as u64;
    for r in &mut pkg.resources {
        let mut data = Vec::new();
        if let Ok(mut f) = z.by_name(format!("{RESOURCES_PREFIX}{}", safe_file_name(&r.file)).as_str()) {
            f.by_ref().take(MAX_RESOURCE + 1).read_to_end(&mut data).map_err(|e| FormatError::Io(e.to_string()))?;
        }
        total = total.saturating_add(data.len() as u64);
        if total > MAX_PACKAGE_BYTES {
            return Err(bad(format!("package resources exceed {MAX_PACKAGE_BYTES} bytes")));
        }
        r.bytes = ResourceBytes::new(data);
    }
    Ok(pkg)
}
