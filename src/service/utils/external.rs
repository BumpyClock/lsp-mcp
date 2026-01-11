// ABOUTME: External package detection utilities for node_modules.
// ABOUTME: Parses package info from npm and pnpm-style paths.

use serde::{Deserialize, Serialize};

/// Information about a package from node_modules
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
}

/// Information about external (node_modules) code
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ExternalInfo {
    pub external: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<PackageInfo>,
}

impl ExternalInfo {
    /// Parses external package info from a file path.
    /// Returns Some if the path contains node_modules, None otherwise.
    pub fn from_path(path: &str) -> Option<Self> {
        if !path.contains("node_modules") {
            return None;
        }

        let package = parse_pnpm_package_info(path).or_else(|| parse_standard_package_info(path));

        Some(ExternalInfo {
            external: true,
            package,
        })
    }
}

/// Parses package info from pnpm-style paths like:
/// node_modules/.pnpm/@reduxjs+toolkit@2.0.0/node_modules/@reduxjs/toolkit/...
pub(crate) fn parse_pnpm_package_info(path: &str) -> Option<PackageInfo> {
    if !path.contains(".pnpm/") {
        return None;
    }

    let pnpm_start = path.find(".pnpm/")?;
    let after_pnpm = &path[pnpm_start + 6..];

    let nm_in_pnpm = after_pnpm.find("/node_modules/")?;
    let package_segment = &after_pnpm[..nm_in_pnpm];

    let after_nm = &after_pnpm[nm_in_pnpm + 14..];
    let name = if after_nm.starts_with('@') {
        let first_slash = after_nm.find('/')?;
        let rest = &after_nm[first_slash + 1..];
        let second_slash = rest.find('/').unwrap_or(rest.len());
        &after_nm[..first_slash + 1 + second_slash]
    } else {
        let slash = after_nm.find('/').unwrap_or(after_nm.len());
        &after_nm[..slash]
    };

    let version_segment = package_segment.split('_').next()?;
    let at_pos = version_segment.rfind('@')?;
    if at_pos == 0 {
        return None;
    }
    let version = &version_segment[at_pos + 1..];

    Some(PackageInfo {
        name: name.to_string(),
        version: version.to_string(),
    })
}

/// Parses package info from standard npm paths like:
/// node_modules/react/index.js or node_modules/@scope/package/index.js
pub(crate) fn parse_standard_package_info(path: &str) -> Option<PackageInfo> {
    let nm_pos = path.find("node_modules/")?;
    let after_nm = &path[nm_pos + 13..];

    let (name, _rest) = if after_nm.starts_with('@') {
        let first_slash = after_nm.find('/')?;
        let second_slash = after_nm[first_slash + 1..]
            .find('/')
            .map(|p| p + first_slash + 1)?;
        (&after_nm[..second_slash], &after_nm[second_slash..])
    } else {
        let slash_pos = after_nm.find('/')?;
        (&after_nm[..slash_pos], &after_nm[slash_pos..])
    };

    Some(PackageInfo {
        name: name.to_string(),
        version: "unknown".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pnpm_package_scoped_with_peer_deps() {
        let path = "node_modules/.pnpm/@reduxjs+toolkit@2.9.1_react-redux@9.2.0_react@18.3.1__react@18.3.1/node_modules/@reduxjs/toolkit/dist/index.js";
        let result = parse_pnpm_package_info(path);
        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.name, "@reduxjs/toolkit");
        assert_eq!(info.version, "2.9.1");
    }

    #[test]
    fn test_parse_pnpm_package_non_scoped_with_peer_deps() {
        let path = "node_modules/.pnpm/lodash@4.17.21_react@18.3.1/node_modules/lodash/index.js";
        let result = parse_pnpm_package_info(path);
        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.name, "lodash");
        assert_eq!(info.version, "4.17.21");
    }

    #[test]
    fn test_parse_pnpm_package_scoped_no_peer_deps() {
        let path = "node_modules/.pnpm/@types+node@20.10.0/node_modules/@types/node/index.d.ts";
        let result = parse_pnpm_package_info(path);
        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.name, "@types/node");
        assert_eq!(info.version, "20.10.0");
    }

    #[test]
    fn test_parse_pnpm_package_non_scoped_no_peer_deps() {
        let path = "node_modules/.pnpm/react@18.3.1/node_modules/react/index.js";
        let result = parse_pnpm_package_info(path);
        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.name, "react");
        assert_eq!(info.version, "18.3.1");
    }

    #[test]
    fn test_parse_pnpm_package_complex_peer_deps() {
        let path = "node_modules/.pnpm/@emotion+react@11.11.0_@types+react@18.2.0_react@18.3.1/node_modules/@emotion/react/dist/index.js";
        let result = parse_pnpm_package_info(path);
        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.name, "@emotion/react");
        assert_eq!(info.version, "11.11.0");
    }

    #[test]
    fn test_parse_standard_package_scoped() {
        let path = "node_modules/@scope/package/index.js";
        let result = parse_standard_package_info(path);
        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.name, "@scope/package");
        assert_eq!(info.version, "unknown");
    }

    #[test]
    fn test_parse_standard_package_non_scoped() {
        let path = "node_modules/react/index.js";
        let result = parse_standard_package_info(path);
        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.name, "react");
        assert_eq!(info.version, "unknown");
    }

    #[test]
    fn test_external_info_from_path_with_node_modules() {
        let path = "node_modules/react/index.js";
        let result = ExternalInfo::from_path(path);
        assert!(result.is_some());
        let info = result.unwrap();
        assert!(info.external);
        assert!(info.package.is_some());
    }

    #[test]
    fn test_external_info_from_path_without_node_modules() {
        let path = "src/components/Button.tsx";
        let result = ExternalInfo::from_path(path);
        assert!(result.is_none());
    }
}
