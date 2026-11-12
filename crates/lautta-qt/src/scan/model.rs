// SPDX-License-Identifier: LGPL-2.1-or-later
//! Pages of one scan for QML. Made when the scan starts, destroyed when it is saved or left: the
//! photos go with it.

use super::rows::{Rows, Step};
use super::worker::{self, Event, Job, Msg};
use crate::json::{from_json, to_json};
use crate::operations::{error_parts, local_path, parse_uri};
use crate::runtime::{core, handle};
use lautta_core::scan::session::ScanSession;
use lautta_core::scan::{Colour, Quad};
use lautta_core::{Error, ErrorKind};
use qmetaobject::prelude::*;
use qmetaobject::QPointer;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::mpsc::UnboundedSender;

const ROLE_ID: i32 = 0x100;
const ROLE_VIEW: i32 = 0x101;
const ROLE_PICTURE: i32 = 0x102;
const ROLE_QUAD: i32 = 0x103;
const ROLE_TURNS: i32 = 0x104;
const ROLE_COLOUR: i32 = 0x105;
const ROLE_FOUND: i32 = 0x106;

struct Running {
    jobs: UnboundedSender<Job>,
    dir: PathBuf,
    shots: u32,
}

#[derive(QObject, Default)]
pub struct ScanModel {
    base: qt_base_class!(trait QAbstractListModel),
    count: qt_property!(i32; NOTIFY countChanged),
    /// Photos or changes still being worked on.
    busy: qt_property!(bool; NOTIFY busyChanged),
    saving: qt_property!(bool; NOTIFY savingChanged),
    /// Of saving, 0..=1.
    progress: qt_property!(f64; NOTIFY progressChanged),
    countChanged: qt_signal!(),
    busyChanged: qt_signal!(),
    savingChanged: qt_signal!(),
    progressChanged: qt_signal!(),

    shotPath: qt_method!(fn(&mut self) -> QString),
    add: qt_method!(fn(&mut self, path: QString)),
    retake: qt_method!(fn(&mut self, index: i32, path: QString)),
    setColour: qt_method!(fn(&mut self, index: i32, colour: QString)),
    turn: qt_method!(fn(&mut self, index: i32, quarters: i32)),
    setCorners: qt_method!(fn(&mut self, index: i32, corners_json: QString)),
    findEdges: qt_method!(fn(&mut self, index: i32)),
    remove: qt_method!(fn(&mut self, index: i32)),
    movePage: qt_method!(fn(&mut self, from: i32, to: i32)),
    save: qt_method!(fn(&mut self, dest_uri: QString, name: QString)),
    pageJson: qt_method!(fn(&self, index: i32) -> QString),

    /// Page at `index` has a new photo: new page, or retaken one.
    photoReady: qt_signal!(index: i32),
    /// `corners` as `setCorners` takes them, empty when the photo shows no page edges.
    edgesSearched: qt_signal!(index: i32, corners: QString),
    saved: qt_signal!(uri: QString),
    failed: qt_signal!(kind: QString, message: QString),

    rows: Rows,
    running: Option<Running>,
    pending: u32,
}

impl QAbstractListModel for ScanModel {
    fn row_count(&self) -> i32 {
        i32::try_from(self.rows.pages.len()).unwrap_or(i32::MAX)
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let Some(p) = usize::try_from(index.row())
            .ok()
            .and_then(|i| self.rows.pages.get(i))
        else {
            return QVariant::default();
        };
        let text = |s: &str| -> QVariant { QString::from(s).into() };
        match role {
            ROLE_ID => p.id.into(),
            ROLE_VIEW => text(&p.view),
            ROLE_PICTURE => text(&p.picture),
            ROLE_QUAD => text(&to_json(&p.quad)),
            ROLE_TURNS => i32::from(p.turns).into(),
            ROLE_COLOUR => text(p.colour),
            ROLE_FOUND => p.found.into(),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        let mut names = HashMap::new();
        names.insert(ROLE_ID, "pageId".into());
        names.insert(ROLE_VIEW, "view".into());
        names.insert(ROLE_PICTURE, "picture".into());
        names.insert(ROLE_QUAD, "corners".into());
        names.insert(ROLE_TURNS, "turns".into());
        names.insert(ROLE_COLOUR, "colour".into());
        names.insert(ROLE_FOUND, "edgesFound".into());
        names
    }
}

impl ScanModel {
    fn report(&self, e: &Error) {
        let (kind, message) = error_parts(e);
        self.failed(kind, message);
    }

    /// Session and its worker, made on first use.
    fn running(&mut self) -> Option<&mut Running> {
        if self.running.is_none() {
            let core = core()?;
            let session = match ScanSession::create(&core.paths) {
                Ok(s) => s,
                Err(e) => {
                    self.report(&e);
                    return None;
                }
            };
            let dir = session.dir().to_path_buf();
            let (jobs, queue) = tokio::sync::mpsc::unbounded_channel();
            let me = QPointer::from(&*self);
            let deliver = qmetaobject::queued_callback(move |msg: Msg| {
                if let Some(p) = me.as_pinned() {
                    p.borrow_mut().arrived(msg);
                }
            });
            handle().spawn(worker::run(core, session, queue, deliver));
            self.running = Some(Running { jobs, dir, shots: 0 });
        }
        self.running.as_mut()
    }

    fn queue(&mut self, job: Job) {
        let Some(running) = self.running() else { return };
        if running.jobs.send(job).is_ok() {
            self.pending += 1;
            self.set_busy();
        }
    }

    fn set_busy(&mut self) {
        let busy = self.pending > 0;
        if self.busy != busy {
            self.busy = busy;
            self.busyChanged();
        }
    }

    fn set_saving(&mut self, saving: bool) {
        self.saving = saving;
        self.savingChanged();
    }

    fn id_at(&self, index: i32) -> Option<u32> {
        let page = self.rows.pages.get(usize::try_from(index).ok()?)?;
        Some(page.id)
    }

    /// Queues `job` for the page at `index`; out of range is ignored: the page was removed while
    /// the tap was on its way.
    fn queue_for(&mut self, index: i32, job: impl FnOnce(u32) -> Job) {
        if let Some(id) = self.id_at(index) {
            self.queue(job(id));
        }
    }

    fn arrived(&mut self, msg: Msg) {
        let (pages, event) = match msg {
            Msg::Progress(share) => {
                self.progress = share;
                return self.progressChanged();
            }
            Msg::Done { pages, event } => (pages, event),
        };
        self.pending = self.pending.saturating_sub(1);
        for step in self.rows.steps(&pages) {
            match step {
                Step::Change(i) => {
                    if let Some(new) = pages.iter().find(|p| p.id == self.rows.pages[i].id) {
                        self.rows.pages[i] = new.clone();
                    }
                    let index = self.row_index(i32::try_from(i).unwrap_or(i32::MAX));
                    self.data_changed(index, index);
                }
                Step::Append(page) => {
                    let at = self.row_count();
                    self.begin_insert_rows(at, at);
                    self.rows.pages.push(page);
                    self.end_insert_rows();
                }
            }
        }
        self.set_count();
        self.set_busy();
        self.publish(event);
    }

    fn set_count(&mut self) {
        let count = self.row_count();
        if self.count != count {
            self.count = count;
            self.countChanged();
        }
    }

    fn index_of(&self, id: u32) -> Option<i32> {
        self.rows.index_of(id).and_then(|i| i32::try_from(i).ok())
    }

    fn publish(&mut self, event: Event) {
        match event {
            Event::Changed => {}
            Event::Photo(id) => {
                if let Some(index) = self.index_of(id) {
                    self.photoReady(index);
                }
            }
            Event::Edges(id, found) => {
                if let Some(index) = self.index_of(id) {
                    let corners = found.map(|c| to_json(&c)).unwrap_or_default();
                    self.edgesSearched(index, QString::from(corners.as_str()));
                }
            }
            Event::Saved(uri) => {
                self.set_saving(false);
                self.saved(QString::from(uri.to_string().as_str()));
            }
            Event::Failed { error, saving } => {
                if saving {
                    self.set_saving(false);
                }
                self.report(&error);
            }
        }
    }

    /// Where the camera writes its next photo.
    fn shotPath(&mut self) -> QString {
        let Some(running) = self.running() else {
            return QString::default();
        };
        running.shots += 1;
        let path = running.dir.join(format!("shot-{}.jpg", running.shots));
        QString::from(path.display().to_string().as_str())
    }

    /// `path` may be a `file://` URL.
    fn add(&mut self, path: QString) {
        self.queue(Job::Add(local_path(&path.to_string())));
    }

    fn retake(&mut self, index: i32, path: QString) {
        let photo = local_path(&path.to_string());
        self.queue_for(index, |id| Job::Retake(id, photo));
    }

    fn setColour(&mut self, index: i32, colour: QString) {
        match Colour::parse(&colour.to_string()) {
            Some(colour) => self.queue_for(index, |id| Job::Colour(id, colour)),
            None => self.report(&Error::new(ErrorKind::InvalidArgument, "bad arguments")),
        }
    }

    fn turn(&mut self, index: i32, quarters: i32) {
        self.queue_for(index, |id| Job::Turn(id, quarters));
    }

    /// `[x0, y0, … x3, y3]`, each 0..1 of the photo, from top left clockwise.
    fn setCorners(&mut self, index: i32, corners_json: QString) {
        let corners: Option<Vec<f32>> = from_json(&corners_json.to_string());
        match corners.and_then(|c| Quad::from_flat(&c)) {
            Some(quad) => self.queue_for(index, |id| Job::Corners(id, quad)),
            None => self.report(&Error::new(ErrorKind::InvalidArgument, "bad arguments")),
        }
    }

    fn findEdges(&mut self, index: i32) {
        self.queue_for(index, Job::FindEdges);
    }

    fn remove(&mut self, index: i32) {
        let Some(id) = self.id_at(index) else { return };
        self.begin_remove_rows(index, index);
        self.rows.forget(usize::try_from(index).unwrap_or_default());
        self.end_remove_rows();
        self.set_count();
        self.queue(Job::Remove(id));
    }

    fn movePage(&mut self, from: i32, to: i32) {
        let (Some(id), Some(_)) = (self.id_at(from), self.id_at(to)) else {
            return;
        };
        if from == to {
            return;
        }
        let (a, b) = (
            usize::try_from(from).unwrap_or_default(),
            usize::try_from(to).unwrap_or_default(),
        );
        let top = QModelIndex::default();
        // Qt names the place before the move: past `to` when moving down.
        self.begin_move_rows(top, from, from, top, if to > from { to + 1 } else { to });
        let page = self.rows.pages.remove(a);
        self.rows.pages.insert(b, page);
        self.end_move_rows();
        self.queue(Job::Move(id, b));
    }

    fn save(&mut self, dest_uri: QString, name: QString) {
        let Some(dest) = parse_uri(&dest_uri) else {
            return self.report(&Error::new(ErrorKind::InvalidArgument, "bad arguments"));
        };
        if self.saving {
            return;
        }
        self.progress = 0.0;
        self.progressChanged();
        self.set_saving(true);
        self.queue(Job::Save {
            dest,
            name: name.to_string(),
        });
    }

    fn pageJson(&self, index: i32) -> QString {
        let page = usize::try_from(index).ok().and_then(|i| self.rows.pages.get(i));
        QString::from(page.map(to_json).unwrap_or_else(|| "null".to_owned()).as_str())
    }
}
