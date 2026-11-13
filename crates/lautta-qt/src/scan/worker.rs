// SPDX-License-Identifier: LGPL-2.1-or-later
//! One task per scan runs its jobs in the order they were queued; picture work on blocking
//! threads. Ends when the queue's sender is dropped, taking the session's files along.

use lautta_core::app::Core;
use lautta_core::scan::pdf::created_now;
use lautta_core::scan::session::{PageInfo, ScanSession};
use lautta_core::scan::{Colour, Quad};
use lautta_core::{Error, ErrorKind, Result, Uri};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::mpsc::UnboundedReceiver;

/// Share of the progress bar for writing pages; the rest is the copy into the folder.
const BUILD_SHARE: f64 = 0.9;

/// Bar while `done` of `total` bytes are in the folder; size unknown: bar waits where writing ended.
fn copy_progress(done: u64, total: Option<u64>) -> f64 {
    let share = total.map_or(0.0, |t| done as f64 / t.max(1) as f64);
    BUILD_SHARE + (1.0 - BUILD_SHARE) * share
}

/// Pages are named by id: places on screen run ahead of the session's.
pub enum Job {
    Add(PathBuf),
    Retake(u32, PathBuf),
    Colour(u32, Colour),
    Turn(u32, i32),
    Corners(u32, Quad),
    Remove(u32),
    Move(u32, usize),
    Save { dest: Uri, name: String },
}

#[derive(Debug)]
pub enum Event {
    Changed,
    /// Page has a new photo: new page, or retaken one.
    Photo(u32),
    Saved(Uri),
    Failed {
        error: Error,
        saving: bool,
    },
}

#[derive(Debug)]
pub enum Msg {
    /// Saving, 0..=1.
    Progress(f64),
    /// One per job.
    Done { pages: Vec<PageInfo>, event: Event },
}

fn lock(session: &Mutex<ScanSession>) -> MutexGuard<'_, ScanSession> {
    session.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn place(session: &ScanSession, id: u32) -> Result<usize> {
    session
        .position(id)
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "no such page"))
}

fn apply(session: &mut ScanSession, job: Job) -> Result<Event> {
    match job {
        Job::Add(photo) => {
            let index = session.add(&photo)?;
            Ok(Event::Photo(session.pages()[index].id))
        }
        Job::Retake(id, photo) => session
            .retake(place(session, id)?, &photo)
            .map(|()| Event::Photo(id)),
        Job::Colour(id, colour) => session
            .set_colour(place(session, id)?, colour)
            .map(|()| Event::Changed),
        Job::Turn(id, quarters) => session
            .turn(place(session, id)?, quarters)
            .map(|()| Event::Changed),
        Job::Corners(id, quad) => session
            .set_quad(place(session, id)?, quad)
            .map(|()| Event::Changed),
        Job::Remove(id) => session.remove(place(session, id)?).map(|()| Event::Changed),
        Job::Move(id, to) => session
            .move_page(place(session, id)?, to)
            .map(|()| Event::Changed),
        Job::Save { .. } => Err(Error::kind(ErrorKind::Internal)),
    }
}

fn join_error(e: tokio::task::JoinError) -> Error {
    Error::new(ErrorKind::Internal, e.to_string())
}

async fn save<D>(
    core: &Core,
    session: &Arc<Mutex<ScanSession>>,
    (dest, name): (Uri, String),
    deliver: &D,
) -> Result<Uri>
where
    D: Fn(Msg) + Send + Sync + Clone + 'static,
{
    let title = name.trim().to_owned();
    let out = lock(session).dir().join("scan.pdf");
    let built = {
        let (session, deliver, out) = (session.clone(), deliver.clone(), out.clone());
        tokio::task::spawn_blocking(move || -> Result<()> {
            let file = std::io::BufWriter::new(std::fs::File::create(out)?);
            let progress =
                |done: usize, of: usize| deliver(Msg::Progress(BUILD_SHARE * done as f64 / of as f64));
            lock(&session)
                .write_pdf(file, &title, &created_now(), &progress)
                .map(|_| ())
        })
        .await
        .map_err(join_error)
        .and_then(|r| r)
    };
    let saved = match built {
        Ok(()) => {
            let deliver = deliver.clone();
            let upload = Arc::new(move |done: u64, total: Option<u64>| {
                deliver(Msg::Progress(copy_progress(done, total)))
            });
            core.save_scan(&out, &dest, &name, upload).await
        }
        Err(e) => Err(e),
    };
    let _ = tokio::task::spawn_blocking(move || std::fs::remove_file(out)).await;
    saved
}

pub async fn run<D>(core: Arc<Core>, session: ScanSession, mut jobs: UnboundedReceiver<Job>, deliver: D)
where
    D: Fn(Msg) + Send + Sync + Clone + 'static,
{
    let session = Arc::new(Mutex::new(session));
    while let Some(job) = jobs.recv().await {
        let (result, saving) = match job {
            Job::Save { dest, name } => {
                let saved = save(&core, &session, (dest, name), &deliver).await;
                (saved.map(Event::Saved), true)
            }
            job => {
                let session = session.clone();
                let applied = tokio::task::spawn_blocking(move || apply(&mut lock(&session), job))
                    .await
                    .map_err(join_error)
                    .and_then(|r| r);
                (applied, false)
            }
        };
        let event = result.unwrap_or_else(|error| Event::Failed { error, saving });
        let pages = lock(&session).pages();
        deliver(Msg::Done { pages, event });
    }
}

#[cfg(test)]
mod tests;
