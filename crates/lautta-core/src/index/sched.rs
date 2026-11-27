// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::bridge::Network;

pub const CHARGE_THRESHOLD: u8 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Conditions {
    pub app_active: bool,
    pub search_open: bool,
    pub charging: bool,
    pub battery: Option<u8>,
    pub index_on_charger: bool,
    pub network: Network,
    pub index_on_mobile: bool,
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

    pub fn waits_for_wifi(&self) -> bool {
        self.network == Network::Mobile && !self.index_on_mobile
    }

    pub fn servers_allowed(&self) -> bool {
        match self.network {
            Network::Wifi | Network::Ethernet => self.app_active || self.on_charger(),
            Network::Mobile => self.index_on_mobile && (self.app_active || self.on_charger()),
            Network::Offline => false,
            Network::Unknown | Network::Other => self.app_active,
        }
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
            ..Conditions::default()
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
    fn without_a_known_network_servers_wait_for_the_app_to_be_open() {
        for network in [Network::Unknown, Network::Other] {
            let on = |c: Conditions| Conditions { network, ..c };
            assert!(on(c(true, false, false, None, false)).servers_allowed());
            assert!(
                !on(c(false, false, true, Some(90), true)).servers_allowed(),
                "{network:?}"
            );
        }
    }

    #[test]
    fn on_wifi_servers_are_indexed_in_front_or_on_the_charger() {
        for network in [Network::Wifi, Network::Ethernet] {
            let on = |c: Conditions| Conditions { network, ..c };
            assert!(on(c(true, false, false, None, false)).servers_allowed());
            assert!(
                on(c(false, false, true, Some(90), true)).servers_allowed(),
                "{network:?}"
            );
            assert!(!on(c(false, false, true, Some(50), true)).servers_allowed());
            assert!(!on(c(false, false, true, Some(90), false)).servers_allowed());
            assert!(!on(c(false, false, false, Some(90), true)).servers_allowed());
        }
    }

    #[test]
    fn on_mobile_data_servers_are_indexed_only_with_the_setting() {
        let mobile = |c: Conditions, index_on_mobile| Conditions {
            network: Network::Mobile,
            index_on_mobile,
            ..c
        };
        assert!(
            !mobile(c(true, true, true, Some(90), true), false).servers_allowed(),
            "off by default"
        );
        assert!(mobile(c(true, false, false, None, false), true).servers_allowed());
        assert!(mobile(c(false, false, true, Some(90), true), true).servers_allowed());
        assert!(!mobile(c(false, false, false, None, false), true).servers_allowed());
        assert!(!Conditions::default().index_on_mobile);
        assert!(mobile(c(true, false, false, None, false), false).waits_for_wifi());
        assert!(!mobile(c(true, false, false, None, false), true).waits_for_wifi());
        assert!(
            !c(true, false, false, None, false).waits_for_wifi(),
            "only mobile data waits"
        );
    }

    #[test]
    fn without_a_network_no_server_is_indexed() {
        let offline = Conditions {
            network: Network::Offline,
            index_on_mobile: true,
            ..c(true, true, true, Some(90), true)
        };
        assert!(!offline.servers_allowed());
        assert_eq!(offline.mode(), Mode::Normal, "the device is indexed all the same");
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
