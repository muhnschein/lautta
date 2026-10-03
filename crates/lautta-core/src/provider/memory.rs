// SPDX-License-Identifier: LGPL-2.1-or-later
//! An in-memory provider: the test double for planners, transfers, search and
//! sync. Capabilities are configurable so tests can exercise capability-driven
//! behaviour (SPEC ARC-4) without a real file system or bridge.

use super::{
    AttributeChanges, CopyOptions, Disposition, Lane, ProgressSink, Provider, ReadHandle, ReadOptions,
    RenameMode, SpaceInfo, WriteOptions,
};
use crate::entry::{cap, Capabilities, Entry, Kind};
use crate::error::{Error, ErrorKind, Result};
use crate::vpath::VPath;
use async_trait::async_trait;
use sha2::Digest;
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::OwnedFd;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
enum Node {
    File {
        data: Vec<u8>,
        mtime: SystemTime,
        mode: u32,
    },
    Dir {
        mtime: SystemTime,
        mode: u32,
    },
    Link {
        target: Vec<u8>,
    },
}

#[derive(Default)]
struct State {
    nodes: BTreeMap<VPath, Node>,
    /// Errors injected per path and operation name (`"list"`, `"stat"`, …).
    failures: HashMap<(String, VPath), Vec<Error>>,
    calls: Vec<String>,
}

/// Thread-safe in-memory tree. The root always exists.
#[derive(Clone)]
pub struct MemoryProvider {
    state: Arc<Mutex<State>>,
    caps: Capabilities,
    batch: usize,
}

impl Default for MemoryProvider {
    fn default() -> Self {
        MemoryProvider::new(Capabilities::with(&[
            cap::WRITE,
            cap::SYMLINKS,
            cap::PERMISSIONS,
            cap::SET_MTIME,
            cap::SERVER_COPY,
            cap::RESUME_UPLOAD,
            cap::RANDOM_READ,
            cap::CHECKSUMS,
            cap::SPACE_INFO,
        ]))
    }
}

impl MemoryProvider {
    pub fn new(caps: Capabilities) -> MemoryProvider {
        let mut state = State::default();
        state.nodes.insert(
            VPath::root(),
            Node::Dir {
                mtime: SystemTime::UNIX_EPOCH,
                mode: 0o755,
            },
        );
        MemoryProvider {
            state: Arc::new(Mutex::new(state)),
            caps,
            batch: 64,
        }
    }

    /// Listing batch size (to test batched delivery).
    pub fn with_batch(mut self, batch: usize) -> MemoryProvider {
        self.batch = batch.max(1);
        self
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    fn vp(path: &str) -> VPath {
        VPath::parse(path.as_bytes()).unwrap_or_default()
    }

    /// Adds a file (and missing parent folders) with an mtime in ms.
    pub fn add_file(&self, path: &str, data: &[u8], mtime_ms: i64) {
        let p = Self::vp(path);
        let mut st = self.lock();
        ensure_parents(&mut st, &p);
        st.nodes.insert(
            p,
            Node::File {
                data: data.to_vec(),
                mtime: crate::entry::ms_to_system_time(mtime_ms),
                mode: 0o644,
            },
        );
    }

    pub fn add_dir(&self, path: &str) {
        let p = Self::vp(path);
        let mut st = self.lock();
        ensure_parents(&mut st, &p);
        st.nodes.entry(p).or_insert(Node::Dir {
            mtime: SystemTime::UNIX_EPOCH,
            mode: 0o755,
        });
    }

    pub fn add_symlink(&self, path: &str, target: &str) {
        let p = Self::vp(path);
        let mut st = self.lock();
        ensure_parents(&mut st, &p);
        st.nodes.insert(
            p,
            Node::Link {
                target: target.as_bytes().to_vec(),
            },
        );
    }

    pub fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        match self.lock().nodes.get(&Self::vp(path)) {
            Some(Node::File { data, .. }) => Some(data.clone()),
            _ => None,
        }
    }

    pub fn exists(&self, path: &str) -> bool {
        self.lock().nodes.contains_key(&Self::vp(path))
    }

    /// All paths, sorted (for assertions).
    pub fn paths(&self) -> Vec<String> {
        self.lock()
            .nodes
            .keys()
            .filter(|p| !p.is_root())
            .map(VPath::display)
            .collect()
    }

    /// Makes the next `op` on `path` fail with `err` (queued, consumed in order).
    pub fn fail_next(&self, op: &str, path: &str, err: Error) {
        self.lock()
            .failures
            .entry((op.to_owned(), Self::vp(path)))
            .or_default()
            .push(err);
    }

    /// Names of operations called so far, as `"op path"`.
    pub fn calls(&self) -> Vec<String> {
        self.lock().calls.clone()
    }

    fn enter(&self, op: &str, path: &VPath) -> Result<()> {
        let mut st = self.lock();
        st.calls.push(format!("{op} {}", path.display()));
        if let Some(queue) = st.failures.get_mut(&(op.to_owned(), path.clone())) {
            if !queue.is_empty() {
                return Err(queue.remove(0));
            }
        }
        Ok(())
    }

    fn require_write(&self) -> Result<()> {
        if self.caps.writable() {
            Ok(())
        } else {
            Err(Error::new(ErrorKind::ReadOnlyFilesystem, "read-only location"))
        }
    }

    fn entry_for(path: &VPath, node: &Node, nodes: &BTreeMap<VPath, Node>, follow: bool) -> Entry {
        let name = path.name().unwrap_or(b"");
        match node {
            Node::File { data, mtime, mode } => {
                let mut e = Entry::new(name, Kind::File);
                e.size = Some(data.len() as u64);
                e.modified = Some(*mtime);
                e.mode = Some(*mode);
                e
            }
            Node::Dir { mtime, mode } => {
                let mut e = Entry::new(name, Kind::Dir);
                e.modified = Some(*mtime);
                e.mode = Some(*mode);
                e
            }
            Node::Link { target } => {
                let resolved = resolve_link(path, target);
                let target_node = resolved.as_ref().and_then(|t| nodes.get(t));
                match (follow, target_node, resolved) {
                    (true, Some(n), Some(t)) if !matches!(n, Node::Link { .. }) => {
                        let mut e = Self::entry_for(&t, n, nodes, false);
                        e.name = name.to_vec();
                        e
                    }
                    (_, tn, _) => {
                        let mut e = Entry::new(name, Kind::Symlink);
                        e.target_kind = match tn {
                            Some(Node::File { .. }) => Kind::File,
                            Some(Node::Dir { .. }) => Kind::Dir,
                            _ => Kind::Unknown,
                        };
                        e.size = Some(target.len() as u64);
                        e
                    }
                }
            }
        }
    }

    fn children(st: &State, dir: &VPath) -> Vec<Entry> {
        st.nodes
            .iter()
            .filter(|(p, _)| !p.is_root() && p.parent().as_ref() == Some(dir))
            .map(|(p, n)| Self::entry_for(p, n, &st.nodes, false))
            .collect()
    }

    fn get_dir(st: &State, path: &VPath) -> Result<()> {
        match st.nodes.get(path) {
            Some(Node::Dir { .. }) => Ok(()),
            Some(_) => Err(Error::kind(ErrorKind::NotADirectory)),
            None => Err(Error::kind(ErrorKind::NotFound)),
        }
    }

    fn check_parent(st: &State, path: &VPath) -> Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| Error::kind(ErrorKind::InvalidArgument))?;
        Self::get_dir(st, &parent)
    }
}

fn ensure_parents(st: &mut State, p: &VPath) {
    let mut cur = p.parent();
    while let Some(dir) = cur {
        st.nodes.entry(dir.clone()).or_insert(Node::Dir {
            mtime: SystemTime::UNIX_EPOCH,
            mode: 0o755,
        });
        cur = dir.parent();
    }
}

fn resolve_link(link: &VPath, target: &[u8]) -> Option<VPath> {
    if target.first() == Some(&b'/') {
        return VPath::parse(target).ok();
    }
    let base = link.parent()?;
    let mut parts: Vec<Vec<u8>> = base.components().map(<[u8]>::to_vec).collect();
    for comp in target.split(|b| *b == b'/') {
        match comp {
            b"" | b"." => {}
            b".." => {
                parts.pop()?;
            }
            c => parts.push(c.to_vec()),
        }
    }
    VPath::parse(&parts.join(&b'/')).ok()
}

struct MemoryReader {
    data: Vec<u8>,
}

#[async_trait]
impl ReadHandle for MemoryReader {
    fn size(&self) -> Option<u64> {
        Some(self.data.len() as u64)
    }

    async fn read_at(&self, offset: u64, max: usize) -> Result<Vec<u8>> {
        let start = usize::try_from(offset).unwrap_or(usize::MAX).min(self.data.len());
        let end = start.saturating_add(max).min(self.data.len());
        Ok(self.data[start..end].to_vec())
    }
}

#[async_trait]
impl Provider for MemoryProvider {
    fn capabilities(&self) -> Capabilities {
        self.caps.clone()
    }

    async fn list(&self, dir: &VPath, _lane: Lane, out: mpsc::Sender<Vec<Entry>>) -> Result<()> {
        self.enter("list", dir)?;
        let entries = {
            let st = self.lock();
            Self::get_dir(&st, dir)?;
            Self::children(&st, dir)
        };
        for chunk in entries.chunks(self.batch) {
            if out.send(chunk.to_vec()).await.is_err() {
                return Err(Error::kind(ErrorKind::Canceled));
            }
        }
        Ok(())
    }

    async fn stat(&self, path: &VPath, follow: bool, _lane: Lane) -> Result<Entry> {
        self.enter("stat", path)?;
        let st = self.lock();
        let node = st
            .nodes
            .get(path)
            .ok_or_else(|| Error::kind(ErrorKind::NotFound))?;
        Ok(Self::entry_for(path, node, &st.nodes, follow))
    }

    async fn read_link(&self, path: &VPath) -> Result<Vec<u8>> {
        self.enter("read_link", path)?;
        match self.lock().nodes.get(path) {
            Some(Node::Link { target }) => Ok(target.clone()),
            Some(_) => Err(Error::kind(ErrorKind::InvalidArgument)),
            None => Err(Error::kind(ErrorKind::NotFound)),
        }
    }

    async fn make_dir(&self, path: &VPath, exclusive: bool) -> Result<()> {
        self.enter("make_dir", path)?;
        self.require_write()?;
        let mut st = self.lock();
        Self::check_parent(&st, path)?;
        match st.nodes.get(path) {
            Some(Node::Dir { .. }) if !exclusive => return Ok(()),
            Some(_) => return Err(Error::kind(ErrorKind::AlreadyExists)),
            None => {}
        }
        st.nodes.insert(
            path.clone(),
            Node::Dir {
                mtime: SystemTime::now(),
                mode: 0o755,
            },
        );
        Ok(())
    }

    async fn make_file(&self, path: &VPath) -> Result<()> {
        self.enter("make_file", path)?;
        self.require_write()?;
        let mut st = self.lock();
        Self::check_parent(&st, path)?;
        if st.nodes.contains_key(path) {
            return Err(Error::kind(ErrorKind::AlreadyExists));
        }
        st.nodes.insert(
            path.clone(),
            Node::File {
                data: Vec::new(),
                mtime: SystemTime::now(),
                mode: 0o644,
            },
        );
        Ok(())
    }

    async fn remove_file(&self, path: &VPath) -> Result<()> {
        self.enter("remove_file", path)?;
        self.require_write()?;
        let mut st = self.lock();
        match st.nodes.get(path) {
            Some(Node::Dir { .. }) => Err(Error::kind(ErrorKind::IsADirectory)),
            Some(_) => {
                st.nodes.remove(path);
                Ok(())
            }
            None => Err(Error::kind(ErrorKind::NotFound)),
        }
    }

    async fn remove_dir(&self, path: &VPath) -> Result<()> {
        self.enter("remove_dir", path)?;
        self.require_write()?;
        let mut st = self.lock();
        Self::get_dir(&st, path)?;
        if path.is_root() {
            return Err(Error::kind(ErrorKind::PermissionDenied));
        }
        if !Self::children(&st, path).is_empty() {
            return Err(Error::kind(ErrorKind::DirectoryNotEmpty));
        }
        st.nodes.remove(path);
        Ok(())
    }

    async fn rename(&self, from: &VPath, to: &VPath, mode: RenameMode) -> Result<()> {
        self.enter("rename", from)?;
        self.require_write()?;
        let mut st = self.lock();
        if !st.nodes.contains_key(from) {
            return Err(Error::kind(ErrorKind::NotFound));
        }
        Self::check_parent(&st, to)?;
        if to.starts_with(from) && to != from {
            return Err(Error::kind(ErrorKind::InvalidArgument));
        }
        if st.nodes.contains_key(to) && from != to {
            if mode == RenameMode::NoReplace {
                return Err(Error::kind(ErrorKind::AlreadyExists));
            }
            st.nodes.retain(|p, _| !p.starts_with(to));
        }
        let moved: Vec<(VPath, Node)> = st
            .nodes
            .iter()
            .filter(|(p, _)| p.starts_with(from))
            .map(|(p, n)| (p.clone(), n.clone()))
            .collect();
        for (p, _) in &moved {
            st.nodes.remove(p);
        }
        for (p, n) in moved {
            let rel = p.strip_prefix(from).unwrap_or_default();
            st.nodes.insert(to.join_path(&rel), n);
        }
        Ok(())
    }

    async fn set_attributes(&self, path: &VPath, changes: AttributeChanges) -> Result<()> {
        self.enter("set_attributes", path)?;
        self.require_write()?;
        if changes.mode.is_some() && !self.caps.has(cap::PERMISSIONS) {
            return Err(Error::kind(ErrorKind::Unsupported));
        }
        let mut st = self.lock();
        match st.nodes.get_mut(path) {
            Some(Node::File { mtime, mode, .. }) | Some(Node::Dir { mtime, mode }) => {
                if let Some(m) = changes.mode {
                    *mode = m;
                }
                if let Some(t) = changes.modified {
                    *mtime = t;
                }
                Ok(())
            }
            Some(Node::Link { .. }) => Err(Error::kind(ErrorKind::Unsupported)),
            None => Err(Error::kind(ErrorKind::NotFound)),
        }
    }

    async fn make_symlink(&self, target: &[u8], link: &VPath) -> Result<()> {
        self.enter("make_symlink", link)?;
        self.require_write()?;
        if !self.caps.has(cap::SYMLINKS) {
            return Err(Error::kind(ErrorKind::Unsupported));
        }
        let mut st = self.lock();
        Self::check_parent(&st, link)?;
        if st.nodes.contains_key(link) {
            return Err(Error::kind(ErrorKind::AlreadyExists));
        }
        st.nodes.insert(
            link.clone(),
            Node::Link {
                target: target.to_vec(),
            },
        );
        Ok(())
    }

    async fn make_hardlink(&self, existing: &VPath, new_path: &VPath) -> Result<()> {
        self.enter("make_hardlink", new_path)?;
        self.require_write()?;
        if !self.caps.has(cap::HARDLINKS) {
            return Err(Error::kind(ErrorKind::Unsupported));
        }
        let mut st = self.lock();
        let node = st
            .nodes
            .get(existing)
            .cloned()
            .ok_or_else(|| Error::kind(ErrorKind::NotFound))?;
        if st.nodes.contains_key(new_path) {
            return Err(Error::kind(ErrorKind::AlreadyExists));
        }
        st.nodes.insert(new_path.clone(), node);
        Ok(())
    }

    async fn open_read(&self, path: &VPath, _lane: Lane) -> Result<Box<dyn ReadHandle>> {
        self.enter("open_read", path)?;
        match self.lock().nodes.get(path) {
            Some(Node::File { data, .. }) => Ok(Box::new(MemoryReader { data: data.clone() })),
            Some(_) => Err(Error::kind(ErrorKind::IsADirectory)),
            None => Err(Error::kind(ErrorKind::NotFound)),
        }
    }

    async fn upload_from(
        &self,
        src: OwnedFd,
        dst: &VPath,
        opts: WriteOptions,
        progress: ProgressSink,
    ) -> Result<()> {
        self.enter("upload_from", dst)?;
        self.require_write()?;
        let mut file = std::fs::File::from(src);
        file.seek(SeekFrom::Start(opts.offset))?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        let mut st = self.lock();
        Self::check_parent(&st, dst)?;
        let existing = match st.nodes.get(dst) {
            Some(Node::File { data, .. }) => Some(data.clone()),
            Some(_) => return Err(Error::kind(ErrorKind::IsADirectory)),
            None => None,
        };
        let content = match (opts.disposition, existing) {
            (Disposition::Create, Some(_)) => return Err(Error::kind(ErrorKind::AlreadyExists)),
            (Disposition::Resume, Some(mut old)) => {
                if !self.caps.has(cap::RESUME_UPLOAD) {
                    return Err(Error::kind(ErrorKind::Unsupported));
                }
                old.truncate(usize::try_from(opts.offset).unwrap_or(usize::MAX));
                old.extend_from_slice(&data);
                old
            }
            _ => data,
        };
        let total = content.len() as u64;
        st.nodes.insert(
            dst.clone(),
            Node::File {
                data: content,
                mtime: opts.modified.unwrap_or_else(SystemTime::now),
                mode: opts.mode.unwrap_or(0o644),
            },
        );
        drop(st);
        progress(total, Some(total));
        Ok(())
    }

    async fn download_into(
        &self,
        src: &VPath,
        dst: OwnedFd,
        opts: ReadOptions,
        progress: ProgressSink,
    ) -> Result<()> {
        self.enter("download_into", src)?;
        let data = match self.lock().nodes.get(src) {
            Some(Node::File { data, .. }) => data.clone(),
            Some(_) => return Err(Error::kind(ErrorKind::IsADirectory)),
            None => return Err(Error::kind(ErrorKind::NotFound)),
        };
        let start = usize::try_from(opts.offset).unwrap_or(usize::MAX).min(data.len());
        let mut file = std::fs::File::from(dst);
        file.seek(SeekFrom::Start(start as u64))?;
        file.write_all(&data[start..])?;
        progress(data.len() as u64, Some(data.len() as u64));
        Ok(())
    }

    async fn server_copy(&self, from: &VPath, to: &VPath, opts: CopyOptions) -> Result<()> {
        self.enter("server_copy", from)?;
        self.require_write()?;
        if !self.caps.has(cap::SERVER_COPY) {
            return Err(Error::kind(ErrorKind::Unsupported));
        }
        let mut st = self.lock();
        let node = st
            .nodes
            .get(from)
            .cloned()
            .ok_or_else(|| Error::kind(ErrorKind::NotFound))?;
        if matches!(node, Node::Dir { .. }) {
            return Err(Error::kind(ErrorKind::IsADirectory));
        }
        Self::check_parent(&st, to)?;
        if st.nodes.contains_key(to) && !opts.replace {
            return Err(Error::kind(ErrorKind::AlreadyExists));
        }
        st.nodes.insert(to.clone(), node);
        Ok(())
    }

    async fn checksum(&self, path: &VPath, algorithm: &str) -> Result<Vec<u8>> {
        self.enter("checksum", path)?;
        if !self.caps.has(cap::CHECKSUMS) {
            return Err(Error::kind(ErrorKind::Unsupported));
        }
        let data = match self.lock().nodes.get(path) {
            Some(Node::File { data, .. }) => data.clone(),
            _ => return Err(Error::kind(ErrorKind::NotFound)),
        };
        match algorithm {
            "sha256" => Ok(sha2::Sha256::digest(data).to_vec()),
            _ => Err(Error::kind(ErrorKind::Unsupported)),
        }
    }

    async fn space(&self, dir: &VPath) -> Result<SpaceInfo> {
        self.enter("space", dir)?;
        let used: u64 = self
            .lock()
            .nodes
            .values()
            .map(|n| match n {
                Node::File { data, .. } => data.len() as u64,
                _ => 0,
            })
            .sum();
        Ok(SpaceInfo {
            free: 1 << 30,
            total: (1 << 30) + used,
            used,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{list_all, no_progress, read_all};

    fn vp(s: &str) -> VPath {
        VPath::parse(s.as_bytes()).unwrap()
    }

    #[tokio::test]
    async fn tree_and_listing() {
        let m = MemoryProvider::default().with_batch(1);
        m.add_file("a/b.txt", b"hello", 1000);
        m.add_dir("a/c");
        let mut names: Vec<String> = list_all(&m, &vp("a"), Lane::Interactive)
            .await
            .unwrap()
            .iter()
            .map(Entry::display_name)
            .collect();
        names.sort();
        assert_eq!(names, vec!["b.txt", "c"]);
        let e = m.stat(&vp("a/b.txt"), true, Lane::Interactive).await.unwrap();
        assert_eq!(e.size, Some(5));
        assert_eq!(e.modified_ms(), Some(1000));
        assert_eq!(
            m.list(&vp("a/b.txt"), Lane::Interactive, mpsc::channel(1).0)
                .await
                .unwrap_err()
                .kind,
            ErrorKind::NotADirectory
        );
    }

    #[tokio::test]
    async fn rename_semantics() {
        let m = MemoryProvider::default();
        m.add_file("x/1", b"1", 0);
        m.add_file("y", b"y", 0);
        let err = m
            .rename(&vp("x/1"), &vp("y"), RenameMode::NoReplace)
            .await
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::AlreadyExists);
        m.rename(&vp("x"), &vp("z"), RenameMode::NoReplace).await.unwrap();
        assert_eq!(m.read_file("z/1").unwrap(), b"1");
        assert!(!m.exists("x"));
        m.rename(&vp("z/1"), &vp("y"), RenameMode::Replace).await.unwrap();
        assert_eq!(m.read_file("y").unwrap(), b"1");
        assert!(m
            .rename(&vp("z"), &vp("z/q"), RenameMode::NoReplace)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn remove_and_create() {
        let m = MemoryProvider::default();
        m.add_file("d/f", b"", 0);
        assert_eq!(
            m.remove_dir(&vp("d")).await.unwrap_err().kind,
            ErrorKind::DirectoryNotEmpty
        );
        assert_eq!(
            m.remove_file(&vp("d")).await.unwrap_err().kind,
            ErrorKind::IsADirectory
        );
        m.remove_file(&vp("d/f")).await.unwrap();
        m.remove_dir(&vp("d")).await.unwrap();
        m.make_dir(&vp("n"), true).await.unwrap();
        assert_eq!(
            m.make_dir(&vp("n"), true).await.unwrap_err().kind,
            ErrorKind::AlreadyExists
        );
        m.make_dir(&vp("n"), false).await.unwrap();
        m.make_file(&vp("n/e")).await.unwrap();
        assert_eq!(
            m.make_file(&vp("n/e")).await.unwrap_err().kind,
            ErrorKind::AlreadyExists
        );
        assert_eq!(
            m.make_file(&vp("missing/e")).await.unwrap_err().kind,
            ErrorKind::NotFound
        );
    }

    #[tokio::test]
    async fn read_only_and_injected_failures() {
        let m = MemoryProvider::new(Capabilities::with(&[cap::READ_ONLY]));
        assert_eq!(
            m.make_dir(&vp("x"), true).await.unwrap_err().kind,
            ErrorKind::ReadOnlyFilesystem
        );
        let m = MemoryProvider::default();
        m.add_file("f", b"abc", 0);
        m.fail_next("stat", "f", Error::kind(ErrorKind::ConnectionLost));
        assert_eq!(
            m.stat(&vp("f"), false, Lane::Bulk).await.unwrap_err().kind,
            ErrorKind::ConnectionLost
        );
        assert!(m.stat(&vp("f"), false, Lane::Bulk).await.is_ok());
        assert_eq!(m.calls(), vec!["stat f", "stat f"]);
    }

    #[tokio::test]
    async fn symlinks() {
        let m = MemoryProvider::default();
        m.add_dir("dir");
        m.add_symlink("l", "dir");
        m.add_symlink("dangling", "nope");
        let e = m.stat(&vp("l"), false, Lane::Interactive).await.unwrap();
        assert!(e.is_symlink() && e.is_dir());
        let e = m.stat(&vp("l"), true, Lane::Interactive).await.unwrap();
        assert_eq!(e.kind, Kind::Dir);
        assert_eq!(e.name, b"l");
        let e = m.stat(&vp("dangling"), true, Lane::Interactive).await.unwrap();
        assert_eq!(e.target_kind, Kind::Unknown);
        assert_eq!(m.read_link(&vp("l")).await.unwrap(), b"dir");
    }

    #[tokio::test]
    async fn byte_movement() {
        let m = MemoryProvider::default();
        m.add_file("src", b"0123456789", 0);
        let h = m.open_read(&vp("src"), Lane::Stream).await.unwrap();
        assert_eq!(h.read_at(3, 4).await.unwrap(), b"3456");
        assert_eq!(read_all(h.as_ref(), 100).await.unwrap(), b"0123456789");

        let tmp = tempfile::tempfile().unwrap();
        m.download_into(
            &vp("src"),
            OwnedFd::from(tmp.try_clone().unwrap()),
            ReadOptions::default(),
            no_progress(),
        )
        .await
        .unwrap();
        let mut local = tmp;
        local.seek(SeekFrom::Start(0)).unwrap();
        let mut got = Vec::new();
        local.read_to_end(&mut got).unwrap();
        assert_eq!(got, b"0123456789");

        m.add_file("partial", b"0123", 0);
        let opts = WriteOptions {
            disposition: Disposition::Resume,
            offset: 4,
            ..WriteOptions::default()
        };
        m.upload_from(
            OwnedFd::from(local.try_clone().unwrap()),
            &vp("partial"),
            opts,
            no_progress(),
        )
        .await
        .unwrap();
        assert_eq!(m.read_file("partial").unwrap(), b"0123456789");
        let err = m
            .upload_from(
                OwnedFd::from(local),
                &vp("partial"),
                WriteOptions::default(),
                no_progress(),
            )
            .await
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::AlreadyExists);
    }

    #[tokio::test]
    async fn server_copy_checksum_space() {
        let m = MemoryProvider::default();
        m.add_file("a", b"abc", 0);
        m.server_copy(&vp("a"), &vp("b"), CopyOptions::default())
            .await
            .unwrap();
        assert_eq!(m.read_file("b").unwrap(), b"abc");
        assert!(m
            .server_copy(&vp("a"), &vp("b"), CopyOptions::default())
            .await
            .is_err());
        let sum = m.checksum(&vp("a"), "sha256").await.unwrap();
        assert_eq!(
            hex::encode(sum),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(m.space(&VPath::root()).await.unwrap().used, 6);
        let noserver = MemoryProvider::new(Capabilities::with(&[cap::WRITE]));
        noserver.add_file("a", b"abc", 0);
        assert_eq!(
            noserver
                .server_copy(&vp("a"), &vp("b"), CopyOptions::default())
                .await
                .unwrap_err()
                .kind,
            ErrorKind::Unsupported
        );
    }
}
