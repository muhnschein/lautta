// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::app_pdf::PdfFailure;
use lautta_pdf::{PageSize, RenderedPage};
use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tokio::sync::Semaphore;

pub const MAX_PAGE_THUMB_WIDTH: u32 = 400;
pub const PAGE_THUMB_CACHE_BYTES: usize = 8 * 1024 * 1024;
const MAX_REFUSED_PAGES: usize = 256;
const TALLEST: f32 = 1.5;

pub fn thumb_width(size: PageSize, asked: u32) -> u32 {
    let width = asked.clamp(1, MAX_PAGE_THUMB_WIDTH);
    let fits = (width as f32 * TALLEST * size.width / size.height.max(1.0)).floor();
    if fits.is_finite() && fits >= 1.0 {
        width.min(fits as u32)
    } else {
        1
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Key {
    version: Arc<str>,
    page: usize,
    width: u32,
}

#[derive(Default)]
struct Kept {
    pages: VecDeque<(Key, Arc<RenderedPage>)>,
    bytes: usize,
    refused: VecDeque<(Arc<str>, usize)>,
}

pub struct PageThumbs {
    gate: Semaphore,
    budget: usize,
    kept: Mutex<Kept>,
}

impl Default for PageThumbs {
    fn default() -> Self {
        PageThumbs::with_budget(PAGE_THUMB_CACHE_BYTES)
    }
}

impl PageThumbs {
    pub fn with_budget(budget: usize) -> Self {
        PageThumbs {
            gate: Semaphore::new(1),
            budget,
            kept: Mutex::default(),
        }
    }

    fn kept(&self) -> MutexGuard<'_, Kept> {
        self.kept.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn is_refused(&self, version: &str, page: usize) -> bool {
        self.kept()
            .refused
            .iter()
            .any(|(v, p)| &**v == version && *p == page)
    }

    pub fn cached_bytes(&self) -> usize {
        self.kept().bytes
    }

    fn cached(&self, key: &Key) -> Option<Arc<RenderedPage>> {
        let mut kept = self.kept();
        let at = kept.pages.iter().position(|(k, _)| k == key)?;
        let hit = kept.pages.remove(at)?;
        let page = Arc::clone(&hit.1);
        kept.pages.push_back(hit);
        Some(page)
    }

    fn store(&self, key: Key, page: &Arc<RenderedPage>) {
        let size = page.rgba.len();
        if size > self.budget {
            return;
        }
        let mut kept = self.kept();
        while kept.bytes + size > self.budget {
            let Some((_, old)) = kept.pages.pop_front() else {
                break;
            };
            kept.bytes -= old.rgba.len();
        }
        kept.bytes += size;
        kept.pages.push_back((key, Arc::clone(page)));
    }

    fn refuse(&self, version: Arc<str>, page: usize) {
        let mut kept = self.kept();
        if kept.refused.len() == MAX_REFUSED_PAGES {
            kept.refused.pop_front();
        }
        kept.refused.push_back((version, page));
    }

    pub async fn make<F>(
        &self,
        version: Arc<str>,
        page: usize,
        width: u32,
        render: impl FnOnce() -> F,
    ) -> Result<Arc<RenderedPage>, PdfFailure>
    where
        F: Future<Output = Result<RenderedPage, PdfFailure>>,
    {
        let key = Key {
            version: Arc::clone(&version),
            page,
            width,
        };
        if let Some(hit) = self.cached(&key) {
            return Ok(hit);
        }
        if self.is_refused(&version, page) {
            return Err(PdfFailure::Pdf(lautta_pdf::PdfError::Invalid));
        }
        let _turn = self.gate.acquire().await.map_err(|e| {
            PdfFailure::File(crate::error::Error::new(
                crate::error::ErrorKind::Internal,
                e.to_string(),
            ))
        })?;
        if let Some(hit) = self.cached(&key) {
            return Ok(hit);
        }
        match render().await {
            Ok(drawn) => {
                let drawn = Arc::new(drawn);
                self.store(key, &drawn);
                Ok(drawn)
            }
            Err(PdfFailure::Pdf(e)) => {
                self.refuse(version, page);
                Err(PdfFailure::Pdf(e))
            }
            Err(f) => Err(f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{Error, ErrorKind};
    use lautta_pdf::PdfError;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn drawn(width: u32, height: u32) -> RenderedPage {
        RenderedPage {
            width,
            height,
            rgba: vec![255; (width * height * 4) as usize],
        }
    }

    fn v(s: &str) -> Arc<str> {
        Arc::from(s)
    }

    #[test]
    fn a_thumbnail_is_at_most_the_cap_wide_and_one_and_a_half_times_as_tall() {
        let a4 = PageSize {
            width: 595.0,
            height: 842.0,
        };
        assert_eq!(thumb_width(a4, 200), 200);
        assert_eq!(thumb_width(a4, 5000), MAX_PAGE_THUMB_WIDTH);
        assert_eq!(thumb_width(a4, 0), 1);
        let strip = PageSize {
            width: 100.0,
            height: 3000.0,
        };
        assert_eq!(
            thumb_width(strip, 300),
            15,
            "a tall strip draws narrow, so its height stays 1.5 widths of the tile"
        );
        let flat = PageSize {
            width: 0.0,
            height: 800.0,
        };
        assert_eq!(thumb_width(flat, 300), 1);
        let none = PageSize {
            width: 300.0,
            height: 0.0,
        };
        assert_eq!(thumb_width(none, 300), 300);
    }

    #[tokio::test]
    async fn a_drawn_page_comes_from_the_cache_the_second_time() {
        let thumbs = PageThumbs::default();
        let runs = AtomicUsize::new(0);
        for _ in 0..3 {
            let page = thumbs
                .make(v("a"), 2, 100, || async {
                    runs.fetch_add(1, Ordering::SeqCst);
                    Ok(drawn(100, 150))
                })
                .await
                .unwrap();
            assert_eq!((page.width, page.height), (100, 150));
        }
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        thumbs
            .make(v("a"), 2, 120, || async {
                runs.fetch_add(1, Ordering::SeqCst);
                Ok(drawn(120, 180))
            })
            .await
            .unwrap();
        thumbs
            .make(v("b"), 2, 100, || async {
                runs.fetch_add(1, Ordering::SeqCst);
                Ok(drawn(100, 150))
            })
            .await
            .unwrap();
        assert_eq!(
            runs.load(Ordering::SeqCst),
            3,
            "another width or another file is drawn"
        );
    }

    #[tokio::test]
    async fn the_cache_keeps_to_its_budget_and_drops_the_least_used_first() {
        let one = 10 * 10 * 4;
        let thumbs = PageThumbs::with_budget(2 * one);
        let runs = AtomicUsize::new(0);
        let make = |page: usize| {
            let thumbs = &thumbs;
            let runs = &runs;
            async move {
                thumbs
                    .make(v("a"), page, 10, || async {
                        runs.fetch_add(1, Ordering::SeqCst);
                        Ok(drawn(10, 10))
                    })
                    .await
                    .unwrap();
            }
        };
        make(0).await;
        make(1).await;
        make(0).await;
        make(2).await;
        assert_eq!(thumbs.cached_bytes(), 2 * one);
        assert_eq!(runs.load(Ordering::SeqCst), 3);
        make(0).await;
        assert_eq!(
            runs.load(Ordering::SeqCst),
            3,
            "page 0 was used last, so it stayed"
        );
        make(1).await;
        assert_eq!(runs.load(Ordering::SeqCst), 4, "page 1 was dropped");
        assert_eq!(thumbs.cached_bytes(), 2 * one);
    }

    #[tokio::test]
    async fn a_page_larger_than_the_budget_is_drawn_but_not_kept() {
        let thumbs = PageThumbs::with_budget(100);
        let page = thumbs
            .make(v("a"), 0, 10, || async { Ok(drawn(10, 10)) })
            .await
            .unwrap();
        assert_eq!(page.width, 10);
        assert_eq!(thumbs.cached_bytes(), 0);
    }

    #[tokio::test]
    async fn a_page_that_cannot_be_drawn_is_refused_and_not_drawn_again() {
        let thumbs = PageThumbs::default();
        let runs = AtomicUsize::new(0);
        for _ in 0..2 {
            let res = thumbs
                .make(v("a"), 4, 100, || async {
                    runs.fetch_add(1, Ordering::SeqCst);
                    Err(PdfFailure::Pdf(PdfError::Invalid))
                })
                .await;
            assert_eq!(res, Err(PdfFailure::Pdf(PdfError::Invalid)));
        }
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert!(thumbs.is_refused("a", 4));
        assert!(!thumbs.is_refused("a", 3) && !thumbs.is_refused("b", 4));
        let other = thumbs.make(v("a"), 4, 50, || async { Ok(drawn(50, 70)) }).await;
        assert!(other.is_err(), "refused at any width");
        let changed = thumbs
            .make(v("b"), 4, 100, || async { Ok(drawn(100, 150)) })
            .await;
        assert!(changed.is_ok(), "a changed file is tried again");
    }

    #[tokio::test]
    async fn a_file_error_is_not_a_verdict_on_the_page() {
        let thumbs = PageThumbs::default();
        let res = thumbs
            .make(v("a"), 0, 100, || async {
                Err(PdfFailure::File(Error::kind(ErrorKind::ConnectionLost)))
            })
            .await;
        assert!(res.is_err());
        assert!(!thumbs.is_refused("a", 0));
        assert!(thumbs
            .make(v("a"), 0, 100, || async { Ok(drawn(100, 150)) })
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn the_refused_list_forgets_the_oldest_past_its_limit() {
        let thumbs = PageThumbs::default();
        for page in 0..=MAX_REFUSED_PAGES {
            let _ = thumbs
                .make(v("a"), page, 10, || async {
                    Err(PdfFailure::Pdf(PdfError::Invalid))
                })
                .await;
        }
        assert!(!thumbs.is_refused("a", 0));
        assert!(thumbs.is_refused("a", 1) && thumbs.is_refused("a", MAX_REFUSED_PAGES));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn pages_are_drawn_one_at_a_time() {
        let thumbs = Arc::new(PageThumbs::default());
        let now = Arc::new(AtomicUsize::new(0));
        let most = Arc::new(AtomicUsize::new(0));
        let (release, wait) = tokio::sync::watch::channel(false);
        let mut tasks = Vec::new();
        for page in 0..4 {
            let (thumbs, now, most, mut wait) = (
                Arc::clone(&thumbs),
                Arc::clone(&now),
                Arc::clone(&most),
                wait.clone(),
            );
            tasks.push(tokio::spawn(async move {
                thumbs
                    .make(v("a"), page, 10, move || async move {
                        let inside = now.fetch_add(1, Ordering::SeqCst) + 1;
                        most.fetch_max(inside, Ordering::SeqCst);
                        let _ = wait.wait_for(|go| *go).await;
                        now.fetch_sub(1, Ordering::SeqCst);
                        Ok(drawn(10, 10))
                    })
                    .await
            }));
        }
        while now.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        release.send(true).unwrap();
        for t in tasks {
            assert!(t.await.unwrap().is_ok());
        }
        assert_eq!(most.load(Ordering::SeqCst), 1);
    }
}
