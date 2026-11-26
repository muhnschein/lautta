// SPDX-License-Identifier: LGPL-2.1-or-later

pub const CHARGE_THRESHOLD: u8 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Conditions {
    pub app_active: bool,
    pub search_open: bool,
    pub charging: bool,
    pub battery: Option<u8>,
    pub index_on_charger: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Paused,
    Idle,
    Normal,
}

impl Conditions {
    pub fn on_charger(&self) -> bool {
        self.index_on_charger && self.charging && self.battery.is_some_and(|b| b > CHARGE_THRESHOLD)
    }

    pub fn mode(&self) -> Mode {
        if self.search_open && self.app_active {
            Mode::Normal
        } else if self.app_active || self.on_charger() {
            Mode::Idle
        } else {
            Mode::Paused
        }
    }

    pub fn servers_allowed(&self) -> bool {
        self.app_active
    }
}

pub const NICE_LOWEST: i32 = 19;

pub fn lower_this_thread() {
    let tid = rustix::thread::gettid();
    if rustix::process::setpriority_process(Some(tid), NICE_LOWEST).is_err() {
        log::debug!("index worker keeps its priority");
    }
}

pub fn nice_of_this_thread() -> Option<i32> {
    rustix::process::getpriority_process(Some(rustix::thread::gettid())).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(
        app_active: bool,
        search_open: bool,
        charging: bool,
        battery: Option<u8>,
        setting: bool,
    ) -> Conditions {
        Conditions {
            app_active,
            search_open,
            charging,
            battery,
            index_on_charger: setting,
        }
    }

    #[test]
    fn indexing_runs_idle_while_open_and_normal_while_searching() {
        assert_eq!(c(true, false, false, None, false).mode(), Mode::Idle);
        assert_eq!(c(true, true, false, None, false).mode(), Mode::Normal);
        assert_eq!(c(false, false, false, Some(90), false).mode(), Mode::Paused);
        assert_eq!(
            c(false, true, false, Some(90), false).mode(),
            Mode::Paused,
            "a search page in the background waits"
        );
    }

    #[test]
    fn in_the_background_only_on_a_charger_above_the_threshold_with_the_setting() {
        assert_eq!(c(false, false, true, Some(51), true).mode(), Mode::Idle);
        assert_eq!(
            c(false, false, true, Some(50), true).mode(),
            Mode::Paused,
            "exactly the threshold is not above it"
        );
        assert_eq!(
            c(false, false, true, None, true).mode(),
            Mode::Paused,
            "an unknown level never counts"
        );
        assert_eq!(c(false, false, false, Some(90), true).mode(), Mode::Paused);
        assert_eq!(
            c(false, false, true, Some(90), false).mode(),
            Mode::Paused,
            "off by default"
        );
    }

    #[test]
    fn servers_wait_for_the_app_to_be_open() {
        assert!(c(true, false, false, None, false).servers_allowed());
        assert!(!c(false, false, true, Some(90), true).servers_allowed());
    }

    #[test]
    fn lowering_a_thread_makes_only_that_thread_nicer() {
        let nice = std::thread::spawn(|| {
            lower_this_thread();
            nice_of_this_thread()
        })
        .join()
        .unwrap();
        assert_eq!(nice, Some(NICE_LOWEST));
        assert!(
            nice_of_this_thread().unwrap() < NICE_LOWEST,
            "other threads keep theirs"
        );
    }
}
