// SPDX-License-Identifier: LGPL-2.1-or-later
//! Scan area on `Core`: puts a finished scan into a folder. Qt-free.

use crate::app::Core;
use crate::app_operations::free_name;
use crate::error::{Error, ErrorKind, Result};
use crate::provider::{list_all, Disposition, Lane, ProgressSink, RenameMode, WriteOptions};
use crate::uri::Uri;
use crate::vpath::validate_name;
use std::collections::HashSet;
use std::os::fd::OwnedFd;
use std::path::Path;

const EXTENSION: &str = ".pdf";

pub fn pdf_file_name(name: &str) -> String {
    let name = name.trim();
    if name.to_lowercase().ends_with(EXTENSION) {
        name.to_owned()
    } else {
        format!("{name}{EXTENSION}")
    }
}

impl Core {
    /// Copies `pdf` into `dest_dir`, device folder or server, as `name` or the next free name.
    /// Written under a hidden name first: a cut upload leaves no half PDF under the real one.
    pub async fn save_scan(
        &self,
        pdf: &Path,
        dest_dir: &Uri,
        name: &str,
        progress: ProgressSink,
    ) -> Result<Uri> {
        let provider = self.provider(&dest_dir.location)?;
        if !provider.capabilities().writable() {
            return Err(Error::kind(ErrorKind::ReadOnlyFilesystem));
        }
        let file_name = pdf_file_name(name);
        validate_name(file_name.as_bytes())?;
        let taken: HashSet<Vec<u8>> = list_all(provider.as_ref(), &dest_dir.path, Lane::Interactive)
            .await?
            .into_iter()
            .map(|e| e.name)
            .collect();
        let final_name = free_name(file_name.as_bytes(), &taken);
        let target = dest_dir.join(&final_name)?;
        let part = dest_dir.join(&[b".", final_name.as_slice(), b".lautta-part"].concat())?;
        let source = pdf.to_path_buf();
        let (file, size) = tokio::task::spawn_blocking(move || -> Result<(std::fs::File, u64)> {
            let file = std::fs::File::open(source)?;
            let size = file.metadata()?.len();
            Ok((file, size))
        })
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))??;
        let opts = WriteOptions {
            disposition: Disposition::Truncate,
            size: Some(size),
            ..WriteOptions::default()
        };
        let mut placed = provider
            .upload_from(OwnedFd::from(file), &part.path, opts, progress)
            .await;
        if placed.is_ok() {
            placed = provider
                .rename(&part.path, &target.path, RenameMode::NoReplace)
                .await;
        }
        if let Err(e) = placed {
            let _ = provider.remove_file(&part.path).await;
            return Err(e);
        }
        self.invalidate(dest_dir);
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_extension_is_added_once() {
        assert_eq!(pdf_file_name("Scan 2026-11-12"), "Scan 2026-11-12.pdf");
        assert_eq!(pdf_file_name("  letter  "), "letter.pdf");
        assert_eq!(pdf_file_name("a.pdf"), "a.pdf");
        assert_eq!(pdf_file_name("A.PDF"), "A.PDF");
        assert_eq!(pdf_file_name("v1.2"), "v1.2.pdf");
        assert_eq!(pdf_file_name(""), ".pdf");
    }
}
