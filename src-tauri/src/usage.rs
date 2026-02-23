use chrono::{DateTime, Utc};
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

pub fn compute_usage_bar(label: &str, bucket: &UsageBucket, window_hours: f64) -> UsageBar {
    let now = Utc::now();
    let resets_at = bucket
        .resets_at
        .parse::<DateTime<Utc>>()
        .unwrap_or(now + chrono::Duration::hours(1));

    let remaining = resets_at - now;
    let seconds_remaining = remaining.num_seconds().max(0) as f64;
    let hours_remaining = seconds_remaining / 3600.0;
    let hours_elapsed = (window_hours - hours_remaining).max(0.0);

    let projected = if hours_elapsed < (10.0 / 60.0) {
        // Less than 10 min elapsed — not enough data to extrapolate
        bucket.utilization
    } else {
        let burn_rate = bucket.utilization / hours_elapsed;
        burn_rate * window_hours
    };

    let is_session = label == "Session";
    let color = if is_session {
        compute_session_color(bucket.utilization, projected)
    } else {
        compute_weekly_color(projected)
    };
    let reset_display = format_reset_time(seconds_remaining, &resets_at);

    UsageBar {
        label: label.to_string(),
        utilization: bucket.utilization,
        resets_at: bucket.resets_at.clone(),
        seconds_remaining,
        projected,
        color,
        reset_display,
    }
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
