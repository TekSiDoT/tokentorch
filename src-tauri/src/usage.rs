use chrono::{DateTime, Local, LocalResult, NaiveDate, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageBucket {
    pub utilization: f64,
    pub resets_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiUsageResponse {
    pub five_hour: Option<UsageBucket>,
    pub seven_day: Option<UsageBucket>,
    pub seven_day_sonnet: Option<UsageBucket>,
    pub seven_day_opus: Option<UsageBucket>,
    pub seven_day_oauth_apps: Option<UsageBucket>,
    pub seven_day_cowork: Option<UsageBucket>,
    pub iguana_necktie: Option<UsageBucket>,
    pub extra_usage: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum UsageColor {
    Green,
    Yellow,
    Red,
    RedBlink,
    Gray,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageBar {
    pub label: String,
    pub utilization: f64,
    pub resets_at: String,
    pub seconds_remaining: f64,
    pub projected: f64,
    pub color: UsageColor,
    pub reset_display: String,
    pub gap_display: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageState {
    pub session: Option<UsageBar>,
    pub weekly: Option<UsageBar>,
    pub last_updated: String,
    pub error: Option<String>,
}

const SESSION_WINDOW_HOURS: f64 = 5.0;
const WEEKLY_WINDOW_HOURS: f64 = 7.0 * 24.0;
const ONLINE_START_HOUR: u32 = 8;
const ONLINE_END_HOUR: u32 = 22;
const SECONDS_PER_HOUR: f64 = 3600.0;
const MIN_PROJECTION_ELAPSED_SECONDS: f64 = 10.0 * 60.0;

pub fn compute_usage_bar(label: &str, bucket: &UsageBucket, window_hours: f64) -> UsageBar {
    compute_usage_bar_at(label, bucket, window_hours, Utc::now())
}

fn compute_usage_bar_at(
    label: &str,
    bucket: &UsageBucket,
    window_hours: f64,
    now: DateTime<Utc>,
) -> UsageBar {
    let resets_at = bucket
        .resets_at
        .parse::<DateTime<Utc>>()
        .unwrap_or(now + chrono::Duration::hours(1));

    let remaining = resets_at - now;
    let seconds_remaining = remaining.num_seconds().max(0) as f64;

    let window_start = resets_at - hours_to_duration(window_hours);
    let elapsed_online_seconds = online_seconds_between(window_start, now);
    let remaining_online_seconds = online_seconds_between(now, resets_at);
    let total_online_window_seconds = elapsed_online_seconds + remaining_online_seconds;

    let projected = if elapsed_online_seconds < MIN_PROJECTION_ELAPSED_SECONDS
        || total_online_window_seconds <= 0.0
    {
        // Less than 10 min of online elapsed time - not enough data to extrapolate.
        bucket.utilization
    } else {
        let burn_rate = bucket.utilization / (elapsed_online_seconds / SECONDS_PER_HOUR);
        burn_rate * (total_online_window_seconds / SECONDS_PER_HOUR)
    };

    let is_session = label == "Session";
    let color = if is_session {
        compute_session_color(bucket.utilization, projected)
    } else {
        compute_weekly_color(projected)
    };
    let reset_display = format_reset_time(seconds_remaining, &resets_at);
    let gap_display = compute_gap_display(bucket.utilization, projected, remaining_online_seconds);

    UsageBar {
        label: label.to_string(),
        utilization: bucket.utilization,
        resets_at: bucket.resets_at.clone(),
        seconds_remaining,
        projected,
        color,
        reset_display,
        gap_display,
    }
}

fn hours_to_duration(hours: f64) -> chrono::Duration {
    let seconds = (hours * SECONDS_PER_HOUR).round().max(0.0) as i64;
    chrono::Duration::seconds(seconds)
}

fn resolve_local_datetime(date: NaiveDate, time: NaiveTime) -> Option<DateTime<Local>> {
    let naive = date.and_time(time);
    match Local.from_local_datetime(&naive) {
        LocalResult::Single(dt) => Some(dt),
        LocalResult::Ambiguous(earlier, _) => Some(earlier),
        LocalResult::None => None,
    }
}

fn online_seconds_between(start: DateTime<Utc>, end: DateTime<Utc>) -> f64 {
    if end <= start {
        return 0.0;
    }

    let start_local = start.with_timezone(&Local);
    let end_local = end.with_timezone(&Local);
    let start_day = start_local.date_naive();
    let end_day = end_local.date_naive();
    let online_start = NaiveTime::from_hms_opt(ONLINE_START_HOUR, 0, 0)
        .expect("online start hour constant must be valid");
    let online_end = NaiveTime::from_hms_opt(ONLINE_END_HOUR, 0, 0)
        .expect("online end hour constant must be valid");

    let mut day = start_day;
    let mut total_seconds = 0.0;

    loop {
        if let (Some(day_online_start), Some(day_online_end)) = (
            resolve_local_datetime(day, online_start),
            resolve_local_datetime(day, online_end),
        ) {
            let segment_start = if day == start_day {
                start_local.max(day_online_start)
            } else {
                day_online_start
            };
            let segment_end = if day == end_day {
                end_local.min(day_online_end)
            } else {
                day_online_end
            };

            if segment_end > segment_start {
                total_seconds += (segment_end - segment_start).num_seconds() as f64;
            }
        }

        if day >= end_day {
            break;
        }
        day = match day.succ_opt() {
            Some(next) => next,
            None => break,
        };
    }

    total_seconds.max(0.0)
}

/// Session: short window, resets fast — only blink when actually limited or wildly over-projected
fn compute_session_color(utilization: f64, projected: f64) -> UsageColor {
    if (utilization > 90.0 && projected > 100.0) || projected > 200.0 {
        UsageColor::RedBlink
    } else if projected > 100.0 {
        UsageColor::Red
    } else if projected > 90.0 {
        UsageColor::Yellow
    } else {
        UsageColor::Green
    }
}

/// Weekly: long window — tighter thresholds
fn compute_weekly_color(projected: f64) -> UsageColor {
    if projected > 100.0 {
        UsageColor::RedBlink
    } else if projected > 95.0 {
        UsageColor::Red
    } else if projected > 90.0 {
        UsageColor::Yellow
    } else {
        UsageColor::Green
    }
}

fn format_reset_time(seconds_remaining: f64, resets_at: &DateTime<Utc>) -> String {
    if seconds_remaining <= 0.0 {
        return "resetting...".to_string();
    }

    let hours = (seconds_remaining / 3600.0).floor() as u64;
    let minutes = ((seconds_remaining % 3600.0) / 60.0).floor() as u64;

    if hours < 24 {
        format!("resets in {}h {}m", hours, minutes)
    } else {
        let local = resets_at.with_timezone(&chrono::Local);
        local.format("resets %a %l:%M %p").to_string()
    }
}

/// How long you'll be rate-limited before the window resets.
/// Only shown when projected > 100%.
fn compute_gap_display(
    utilization: f64,
    projected: f64,
    online_seconds_remaining: f64,
) -> Option<String> {
    if projected <= 100.0 || online_seconds_remaining <= 0.0 {
        return None;
    }

    let gap_secs = if utilization >= 100.0 {
        // Already at the limit - gap is the full online remaining time.
        online_seconds_remaining
    } else {
        // gap = remaining * (projected - 100) / (projected - utilization)
        online_seconds_remaining * (projected - 100.0) / (projected - utilization)
    };

    let gap_secs = gap_secs.max(0.0);
    let hours = (gap_secs / 3600.0).floor() as u64;
    let minutes = ((gap_secs % 3600.0) / 60.0).ceil() as u64;

    let time = if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes.max(1))
    };

    Some(format!("{} gap", time))
}

pub fn compute_state(response: &ApiUsageResponse) -> UsageState {
    let session = response
        .five_hour
        .as_ref()
        .map(|b| compute_usage_bar("Session", b, SESSION_WINDOW_HOURS));

    let weekly = response
        .seven_day
        .as_ref()
        .map(|b| compute_usage_bar("Weekly", b, WEEKLY_WINDOW_HOURS));

    UsageState {
        session,
        weekly,
        last_updated: Utc::now().to_rfc3339(),
        error: None,
    }
}

pub fn worst_color(state: &UsageState) -> UsageColor {
    let colors: Vec<UsageColor> = [&state.session, &state.weekly]
        .iter()
        .filter_map(|b| b.as_ref().map(|bar| bar.color))
        .collect();

    if colors.contains(&UsageColor::RedBlink) {
        UsageColor::RedBlink
    } else if colors.contains(&UsageColor::Red) {
        UsageColor::Red
    } else if colors.contains(&UsageColor::Yellow) {
        UsageColor::Yellow
    } else if colors.is_empty() {
        UsageColor::Gray
    } else {
        UsageColor::Green
    }
}

pub fn tray_title(state: &UsageState) -> String {
    let s = state
        .session
        .as_ref()
        .map(|b| format!("S:{:.0}", b.utilization))
        .unwrap_or_else(|| "S:--".to_string());
    let w = state
        .weekly
        .as_ref()
        .map(|b| format!("W:{:.0}", b.utilization))
        .unwrap_or_else(|| "W:--".to_string());
    format!("{} {}", s, w)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_to_utc(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        match Local.with_ymd_and_hms(year, month, day, hour, minute, 0) {
            LocalResult::Single(dt) => dt.with_timezone(&Utc),
            LocalResult::Ambiguous(earlier, _) => earlier.with_timezone(&Utc),
            LocalResult::None => {
                panic!("invalid local datetime in test inputs")
            }
        }
    }

    fn bucket(utilization: f64, resets_at: DateTime<Utc>) -> UsageBucket {
        UsageBucket {
            utilization,
            resets_at: resets_at.to_rfc3339(),
        }
    }

    fn assert_approx(left: f64, right: f64) {
        assert!(
            (left - right).abs() < 0.1,
            "values differ: left={left} right={right}"
        );
    }

    #[test]
    fn online_seconds_skip_offline_overnight() {
        let start = local_to_utc(2026, 1, 15, 21, 0);
        let end = local_to_utc(2026, 1, 16, 9, 0);

        assert_approx(online_seconds_between(start, end), 2.0 * SECONDS_PER_HOUR);
    }

    #[test]
    fn projection_uses_online_time_only() {
        let now = local_to_utc(2026, 1, 15, 21, 0);
        let reset = local_to_utc(2026, 1, 16, 9, 0);
        let usage = bucket(60.0, reset);

        let bar = compute_usage_bar_at("Weekly", &usage, 24.0, now);

        assert_approx(bar.projected, 70.0);
        assert!(bar.projected < 100.0);
    }

    #[test]
    fn gap_display_uses_online_remaining_time() {
        let now = local_to_utc(2026, 1, 15, 21, 0);
        let reset = local_to_utc(2026, 1, 16, 9, 0);
        let usage = bucket(96.0, reset);

        let bar = compute_usage_bar_at("Weekly", &usage, 24.0, now);

        assert_eq!(bar.gap_display.as_deref(), Some("1h 30m gap"));
    }

    #[test]
    fn reset_display_stays_wall_clock_time() {
        let now = local_to_utc(2026, 1, 15, 21, 0);
        let reset = local_to_utc(2026, 1, 16, 9, 0);
        let usage = bucket(96.0, reset);

        let bar = compute_usage_bar_at("Weekly", &usage, 24.0, now);

        assert_eq!(bar.reset_display, "resets in 12h 0m");
    }

    #[test]
    fn projection_waits_for_ten_online_minutes() {
        let now = local_to_utc(2026, 1, 15, 8, 5);
        let reset = local_to_utc(2026, 1, 15, 13, 0);
        let usage = bucket(12.0, reset);

        let bar = compute_usage_bar_at("Session", &usage, 5.0, now);

        assert_eq!(bar.projected, 12.0);
    }
}
