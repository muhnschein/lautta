// SPDX-License-Identifier: LGPL-2.1-or-later
use super::*;
use lautta_core::locations::LocationRegistry;
use lautta_core::paths::AppPaths;
use lautta_pdf::PdfDocument;
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

/// Hang guard: jobs take milliseconds.
const LONG: Duration = Duration::from_secs(60);

struct Bench {
    home: tempfile::TempDir,
    dir: PathBuf,
    jobs: Option<UnboundedSender<Job>>,
    msgs: UnboundedReceiver<Msg>,
    task: tokio::task::JoinHandle<()>,
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/qml/host/fixtures")
        .join(name)
}

async fn bench() -> Bench {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join("Documents")).unwrap();
    let paths = AppPaths::new(home.path());
    let registry = LocationRegistry::with_media_root(paths.clone(), home.path().join("media"));
    let core = Core::open_with(paths.clone(), registry).await.unwrap();
    let session = ScanSession::create(&paths).unwrap();
    let dir = session.dir().to_path_buf();
    let (jobs, queue) = unbounded_channel();
    let (out, msgs) = unbounded_channel();
    let deliver = move |msg: Msg| {
        let _ = out.send(msg);
    };
    let task = tokio::spawn(run(core, session, queue, deliver));
    Bench {
        home,
        dir,
        jobs: Some(jobs),
        msgs,
        task,
    }
}

impl Bench {
    fn send(&self, job: Job) {
        self.jobs.as_ref().unwrap().send(job).ok().unwrap();
    }

    async fn msg(&mut self) -> Msg {
        tokio::time::timeout(LONG, self.msgs.recv())
            .await
            .expect("worker answers")
            .expect("worker alive")
    }

    /// Next job's outcome and the progress reported before it.
    async fn done(&mut self) -> (Vec<PageInfo>, Event, Vec<f64>) {
        let mut progress = Vec::new();
        loop {
            match self.msg().await {
                Msg::Progress(p) => progress.push(p),
                Msg::Done { pages, event } => return (pages, event, progress),
            }
        }
    }

    fn save(&self, name: &str) {
        self.send(Job::Save {
            dest: Uri::parse("lautta://user-documents/").unwrap(),
            name: name.to_owned(),
        });
    }

    fn documents(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.home.path().join("Documents"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

fn ids(pages: &[PageInfo]) -> Vec<u32> {
    pages.iter().map(|p| p.id).collect()
}

#[tokio::test]
async fn jobs_run_in_queue_order_and_each_reports_the_pages() {
    let mut b = bench().await;
    b.send(Job::Add(fixture("sheet.jpg")));
    b.send(Job::Add(fixture("large-photo.jpg")));
    b.send(Job::Colour(1, Colour::Grayscale));
    b.send(Job::Turn(2, 1));
    b.send(Job::Move(2, 0));
    b.send(Job::Remove(1));
    let (pages, event, _) = b.done().await;
    assert!(matches!(event, Event::Photo(1)), "{event:?}");
    assert_eq!(ids(&pages), [1]);
    assert!(pages[0].found, "sheet on a table");
    let (pages, event, _) = b.done().await;
    assert!(matches!(event, Event::Photo(2)), "{event:?}");
    assert_eq!(ids(&pages), [1, 2]);
    let (pages, event, _) = b.done().await;
    assert!(matches!(event, Event::Changed), "{event:?}");
    assert_eq!((pages[0].colour, pages[1].colour), ("grayscale", "original"));
    let (pages, _, _) = b.done().await;
    assert_eq!((pages[0].turns, pages[1].turns), (0, 1));
    let (pages, _, _) = b.done().await;
    assert_eq!(ids(&pages), [2, 1]);
    let (pages, event, _) = b.done().await;
    assert!(matches!(event, Event::Changed), "{event:?}");
    assert_eq!(ids(&pages), [2]);
}

#[tokio::test]
async fn corners_and_edge_search_apply_to_the_named_page() {
    let mut b = bench().await;
    b.send(Job::Add(fixture("large-photo.jpg")));
    b.send(Job::Add(fixture("sheet.jpg")));
    b.done().await;
    b.done().await;
    let half = Quad::from_flat(&[0.0, 0.0, 0.5, 0.0, 0.5, 1.0, 0.0, 1.0]).unwrap();
    b.send(Job::Corners(2, half));
    let (pages, event, _) = b.done().await;
    assert!(matches!(event, Event::Changed), "{event:?}");
    assert_eq!(pages[1].quad, half.to_flat());
    assert!(pages[0].quad != half.to_flat());
    b.send(Job::FindEdges(2));
    let (pages, event, _) = b.done().await;
    let Event::Edges(2, Some(found)) = event else {
        panic!("{event:?}")
    };
    assert!(found != half.to_flat() && Quad::from_flat(&found).is_some());
    assert_eq!(pages[1].quad, half.to_flat());
    b.send(Job::FindEdges(1));
    let (pages, event, _) = b.done().await;
    assert!(matches!(event, Event::Edges(1, None)), "{event:?}");
    assert!(pages[0].quad != half.to_flat());
}

#[tokio::test]
async fn retake_swaps_the_photo_of_the_named_page() {
    let mut b = bench().await;
    b.send(Job::Add(fixture("large-photo.jpg")));
    b.send(Job::Add(fixture("large-photo.jpg")));
    b.send(Job::Retake(2, fixture("sheet.jpg")));
    b.done().await;
    b.done().await;
    let (pages, event, _) = b.done().await;
    assert!(matches!(event, Event::Photo(2)), "{event:?}");
    assert_eq!(ids(&pages), [1, 2]);
    assert!(!pages[0].found && pages[1].found);
}

#[tokio::test]
async fn failed_jobs_report_the_error_and_the_queue_goes_on() {
    let mut b = bench().await;
    let junk = b.home.path().join("junk.jpg");
    std::fs::write(&junk, b"no picture").unwrap();
    b.send(Job::Add(junk));
    b.send(Job::Colour(7, Colour::Grayscale));
    b.send(Job::Add(fixture("photo.jpg")));
    let (pages, event, _) = b.done().await;
    assert!(
        matches!(&event, Event::Failed { error, saving: false } if error.kind == ErrorKind::Unsupported),
        "{event:?}"
    );
    assert!(pages.is_empty());
    let (_, event, _) = b.done().await;
    assert!(
        matches!(&event, Event::Failed { error, saving: false } if error.kind == ErrorKind::NotFound),
        "{event:?}"
    );
    let (pages, event, _) = b.done().await;
    assert!(matches!(event, Event::Photo(1)), "{event:?}");
    assert_eq!(ids(&pages), [1]);
}

#[tokio::test]
async fn save_writes_the_pdf_into_the_folder_with_rising_progress() {
    let mut b = bench().await;
    b.send(Job::Add(fixture("sheet.jpg")));
    b.send(Job::Add(fixture("large-photo.jpg")));
    b.save(" Scan 1 ");
    b.done().await;
    b.done().await;
    let (pages, event, progress) = b.done().await;
    let Event::Saved(uri) = event else {
        panic!("{event:?}")
    };
    assert_eq!(uri.to_string(), "lautta://user-documents/Scan%201.pdf");
    assert_eq!(ids(&pages), [1, 2], "pages stay until the scan is closed");
    assert_eq!(b.documents(), ["Scan 1.pdf"]);
    let pdf = std::fs::read(b.home.path().join("Documents/Scan 1.pdf")).unwrap();
    assert!(String::from_utf8_lossy(&pdf).contains("/Title <FEFF005300630061006E00200031>"));
    assert_eq!(PdfDocument::open(pdf).unwrap().page_count(), 2);
    assert_eq!(&progress[..2], [0.45, 0.9], "one step per page");
    assert!(progress.windows(2).all(|w| w[0] <= w[1]), "{progress:?}");
    assert_eq!(progress.last(), Some(&1.0));
    assert!(!b.dir.join("scan.pdf").exists(), "built file removed");
}

#[test]
fn copy_fills_the_rest_of_the_bar_by_bytes() {
    let near = |got: f64, want: f64| (got - want).abs() < 1e-9;
    assert!(
        near(copy_progress(1, Some(4)), 0.925),
        "{}",
        copy_progress(1, Some(4))
    );
    assert!(near(copy_progress(4, Some(4)), 1.0));
    assert!(near(copy_progress(7, None), 0.9), "size unknown");
    assert!(near(copy_progress(0, Some(0)), 0.9), "empty file");
}

#[tokio::test]
async fn empty_save_fails_as_saving_and_writes_nothing() {
    let mut b = bench().await;
    b.save("none");
    let (_, event, progress) = b.done().await;
    assert!(
        matches!(&event, Event::Failed { error, saving: true } if error.kind == ErrorKind::InvalidArgument),
        "{event:?}"
    );
    assert!(progress.is_empty());
    assert!(b.documents().is_empty());
    assert!(!b.dir.join("scan.pdf").exists());
}

#[tokio::test]
async fn failed_save_removes_the_built_file() {
    let mut b = bench().await;
    b.send(Job::Add(fixture("photo.jpg")));
    b.send(Job::Save {
        dest: Uri::parse("lautta://user-documents/Missing").unwrap(),
        name: "x".to_owned(),
    });
    b.done().await;
    let (_, event, _) = b.done().await;
    assert!(
        matches!(&event, Event::Failed { error, saving: true } if error.kind == ErrorKind::NotFound),
        "{event:?}"
    );
    assert!(!b.dir.join("scan.pdf").exists());
}

#[tokio::test]
async fn closing_the_queue_ends_the_worker_and_removes_the_photos() {
    let mut b = bench().await;
    b.send(Job::Add(fixture("photo.jpg")));
    b.done().await;
    assert!(b.dir.join("photo-1.jpg").exists());
    b.jobs = None;
    tokio::time::timeout(LONG, &mut b.task)
        .await
        .expect("worker ends")
        .unwrap();
    assert!(!b.dir.exists());
}
