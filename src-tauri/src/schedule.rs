//! Pure polling policy; window state and active scope are supplied by the shell.
#[derive(Debug, PartialEq, Eq)]
pub enum Due {
    Sweep,
    Active,
    Prices,
    Idle,
}
pub struct Polling {
    pub sweep_at: i64,
    pub active_at: i64,
    pub prices_at: i64,
    pub sweep_minutes: u32,
    pub price_seconds: u32,
    pub accounts: usize,
    pub active: usize,
    pub minimized: bool,
}
impl Polling {
    pub fn due(&self, now: i64) -> Due {
        let sweep_interval =
            i64::from(self.sweep_minutes.max(5)) * 60 * self.accounts.div_ceil(50).max(1) as i64;
        let active_interval = 300 * self.active.div_ceil(10).max(1) as i64;
        let prices_interval = i64::from(if self.minimized {
            self.price_seconds.max(300)
        } else {
            self.price_seconds.max(15)
        });
        // A full sweep includes/prioritizes the active scope, so one job covers both deadlines.
        if now - self.sweep_at >= sweep_interval {
            Due::Sweep
        } else if !self.minimized && self.active > 0 && now - self.active_at >= active_interval {
            Due::Active
        } else if now - self.prices_at >= prices_interval {
            Due::Prices
        } else {
            Due::Idle
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn polling(accounts: usize, active: usize, minimized: bool) -> Polling {
        Polling {
            sweep_at: 0,
            active_at: 0,
            prices_at: 0,
            sweep_minutes: 60,
            price_seconds: 60,
            accounts,
            active,
            minimized,
        }
    }
    #[test]
    fn minimized_reduces_prices_and_skips_active_polling() {
        assert_eq!(polling(2, 2, true).due(100), Due::Idle);
        assert_eq!(polling(2, 2, true).due(301), Due::Prices);
        assert_eq!(polling(2, 2, false).due(301), Due::Active);
    }
    #[test]
    fn large_profiles_extend_intervals_and_do_not_burst() {
        assert_eq!(
            Polling {
                active_at: 4000,
                prices_at: 4000,
                ..polling(100, 0, false)
            }
            .due(4000),
            Due::Idle
        );
        assert_eq!(
            Polling {
                active_at: 7200,
                prices_at: 7200,
                ..polling(100, 0, false)
            }
            .due(7200),
            Due::Sweep
        );
        assert_eq!(
            Polling {
                sweep_at: 7200,
                active_at: 7200,
                prices_at: 7200,
                ..polling(100, 0, false)
            }
            .due(7201),
            Due::Idle
        );
        assert_eq!(
            Polling {
                prices_at: 300,
                ..polling(50, 50, false)
            }
            .due(300),
            Due::Idle
        );
        assert_eq!(polling(2, 2, false).due(3600), Due::Sweep);
    }
}
