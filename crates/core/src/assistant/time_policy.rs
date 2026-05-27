#[derive(Debug, Clone, PartialEq)]
pub enum WakeMode {
    Proactive,
    Passive,
    Immediate,
}

#[derive(Debug, Clone)]
pub struct TimePolicy {
    pub wake_mode: WakeMode,
    pub summary_interval_secs: u64,
    pub escalation_timeout_secs: u64,
}

#[derive(Debug, Clone)]
pub struct TimePolicyConfig {
    pub busy_start_hour: u32,
    pub busy_end_hour: u32,
    pub busy_days: Vec<u8>, // 0=Mon, 6=Sun
    pub urgent_keywords: Vec<String>,
}

impl Default for TimePolicyConfig {
    fn default() -> Self {
        Self {
            busy_start_hour: 9,
            busy_end_hour: 18,
            busy_days: vec![0, 1, 2, 3, 4], // Mon-Fri
            urgent_keywords: vec![
                "紧急".into(),
                "线上故障".into(),
                "P0".into(),
                "crash".into(),
                "urgent".into(),
            ],
        }
    }
}

impl TimePolicyConfig {
    pub fn resolve(&self, now_secs_since_epoch: u64, message: &str) -> TimePolicy {
        // Check urgent keywords first (overrides everything)
        if self.urgent_keywords.iter().any(|k| message.contains(k.as_str())) {
            return TimePolicy {
                wake_mode: WakeMode::Immediate,
                summary_interval_secs: 900,     // 15min
                escalation_timeout_secs: 300,   // 5min
            };
        }

        // Convert epoch seconds to date/time components
        let secs_per_day: u64 = 86400;
        let days_since_epoch = now_secs_since_epoch / secs_per_day;
        let time_of_day_secs = now_secs_since_epoch % secs_per_day;
        let hour = (time_of_day_secs / 3600) as u32;
        // Weekday: Jan 1, 1970 was Thursday (day 3)
        let weekday = ((days_since_epoch + 3) % 7) as u8;

        let is_busy_day = self.busy_days.contains(&weekday);
        let is_busy_hour = hour >= self.busy_start_hour && hour < self.busy_end_hour;

        if is_busy_day && is_busy_hour {
            TimePolicy {
                wake_mode: WakeMode::Proactive,
                summary_interval_secs: 900,      // 15min
                escalation_timeout_secs: 600,    // 10min
            }
        } else {
            TimePolicy {
                wake_mode: WakeMode::Passive,
                summary_interval_secs: 21600,    // 6h
                escalation_timeout_secs: 7200,   // 2h
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference: Jan 1, 1970 was Thursday (weekday 3)
    // Mon = 0, Tue = 1, Wed = 2, Thu = 3, Fri = 4, Sat = 5, Sun = 6

    #[test]
    fn test_urgent_overrides_idle() {
        let config = TimePolicyConfig::default();
        // Sunday 2026-05-24 22:00:00 UTC
        let sun_22h: u64 = 1779928800;
        let policy = config.resolve(sun_22h, "紧急！线上挂了");
        assert_eq!(policy.wake_mode, WakeMode::Immediate);
    }

    #[test]
    fn test_busy_time_proactive() {
        let config = TimePolicyConfig::default();
        // Monday 2026-05-25 14:00:00 UTC
        let mon_14h: u64 = 1779976800;
        let policy = config.resolve(mon_14h, "帮我写个功能");
        assert_eq!(policy.wake_mode, WakeMode::Proactive);
    }

    #[test]
    fn test_idle_time_passive() {
        let config = TimePolicyConfig::default();
        // Sunday 2026-05-24 22:00:00 UTC
        let sun_22h: u64 = 1779928800;
        let policy = config.resolve(sun_22h, "帮我写个功能");
        assert_eq!(policy.wake_mode, WakeMode::Passive);
    }

    #[test]
    fn test_weekday_calculation_monday() {
        let config = TimePolicyConfig::default();
        // Monday 2026-05-25 10:00:00 UTC
        let mon_10h: u64 = 1779962400;
        let policy = config.resolve(mon_10h, "普通需求");
        assert_eq!(policy.wake_mode, WakeMode::Proactive);
    }

    #[test]
    fn test_after_hours_passive() {
        let config = TimePolicyConfig::default();
        // Monday 2026-05-25 20:00:00 UTC — after 18:00
        let mon_20h: u64 = 1779998400;
        let policy = config.resolve(mon_20h, "普通需求");
        assert_eq!(policy.wake_mode, WakeMode::Passive);
    }

    #[test]
    fn test_weekend_passive() {
        let config = TimePolicyConfig::default();
        // Saturday 2026-05-30 14:00:00 UTC
        let sat_14h: u64 = 1780423200;
        let policy = config.resolve(sat_14h, "普通需求");
        assert_eq!(policy.wake_mode, WakeMode::Passive);
    }

    #[test]
    fn test_urgent_english_keyword() {
        let config = TimePolicyConfig::default();
        let mon_14h: u64 = 1779976800;
        let policy = config.resolve(mon_14h, "urgent bug fix needed");
        assert_eq!(policy.wake_mode, WakeMode::Immediate);
    }

    #[test]
    fn test_monday_morning_busy() {
        let config = TimePolicyConfig::default();
        let mon_09h: u64 = 1779958800;
        let policy = config.resolve(mon_09h, "需求分析");
        assert_eq!(policy.wake_mode, WakeMode::Proactive);
    }

    #[test]
    fn test_friday_evening_idle() {
        let config = TimePolicyConfig::default();
        // Friday 2026-05-29 19:00:00 UTC
        let fri_19h: u64 = 1780268400;
        let policy = config.resolve(fri_19h, "普通任务");
        assert_eq!(policy.wake_mode, WakeMode::Passive);
    }
}
