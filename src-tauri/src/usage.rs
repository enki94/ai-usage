use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Claude,
    Codex,
    Antigravity,
}
impl Provider {
    pub const ALL: [Self; 3] = [Self::Claude, Self::Codex, Self::Antigravity];
    pub fn short(self) -> &'static str {
        match self {
            Self::Claude => "C",
            Self::Codex => "X",
            Self::Antigravity => "A",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Antigravity => "Antigravity",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Antigravity => "antigravity",
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    pub id: String,
    pub group: Option<String>,
    pub label: String,
    pub used: f64,
    pub reset_at: Option<DateTime<Utc>>,
}
impl UsageWindow {
    pub fn bounded_used(&self) -> f64 {
        if self.used.is_finite() {
            self.used.clamp(0.0, 100.0)
        } else {
            0.0
        }
    }
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub windows: Vec<UsageWindow>,
    pub updated_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub is_loading: bool,
}
impl Snapshot {
    pub fn headline(&self) -> Option<f64> {
        let short = self.windows.iter().any(|w| w.label == "5시간");
        self.windows
            .iter()
            .filter(|w| !short || w.label == "5시간")
            .map(UsageWindow::bounded_used)
            .reduce(f64::max)
    }
    pub fn apply(&mut self, result: Result<Vec<UsageWindow>, String>) {
        self.is_loading = false;
        match result {
            Ok(windows) => {
                self.windows = windows;
                self.updated_at = Some(Utc::now());
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
}
pub fn title(snapshots: &BTreeMap<Provider, Snapshot>, selection: &str) -> String {
    Provider::ALL
        .into_iter()
        .filter(|p| selection == "all" || selection == p.key())
        .map(|p| {
            let state = &snapshots[&p];
            let value = state
                .headline()
                .map(|v| format!("{:.0}%", v.round()))
                .unwrap_or_else(|| if state.error.is_some() { "!" } else { "—" }.into());
            let stale = if state.headline().is_some() && state.error.is_some() {
                "*"
            } else {
                ""
            };
            format!("{} {}{}", p.short(), value, stale)
        })
        .collect::<Vec<_>>()
        .join("  ")
}
pub fn date(value: &Value) -> Option<DateTime<Utc>> {
    if let Some(epoch) = value.as_f64() {
        if epoch.is_finite() && epoch >= i64::MIN as f64 && epoch < i64::MAX as f64 {
            let seconds = epoch.floor() as i64;
            let nanos = ((epoch - seconds as f64) * 1_000_000_000.0)
                .round()
                .clamp(0.0, 999_999_999.0) as u32;
            return Utc.timestamp_opt(seconds, nanos).single();
        }
    }
    value
        .as_str()
        .and_then(|v| DateTime::parse_from_rfc3339(v).ok())
        .map(|v| v.with_timezone(&Utc))
}
pub fn parse(json: &Value, provider: Provider) -> Result<Vec<UsageWindow>, String> {
    if !json.is_object() {
        return Err("사용량 응답 형식을 확인할 수 없습니다.".into());
    }
    let mut result = vec![];
    match provider {
        Provider::Claude | Provider::Codex => {
            let codex = provider == Provider::Codex;
            let root = if codex { &json["rate_limit"] } else { json };
            let keys = if codex {
                ["primary_window", "secondary_window"]
            } else {
                ["five_hour", "seven_day"]
            };
            for (i, key) in keys.into_iter().enumerate() {
                let window = &root[key];
                if let Some(used) = window[if codex { "used_percent" } else { "utilization" }]
                    .as_f64()
                    .filter(|n| n.is_finite())
                {
                    result.push(UsageWindow {
                        id: key.into(),
                        group: None,
                        label: if i == 0 { "5시간" } else { "주간" }.into(),
                        used,
                        reset_at: date(&window[if codex { "reset_at" } else { "resets_at" }]),
                    });
                }
            }
        }
        Provider::Antigravity => {
            if let Some(groups) = json["groups"].as_array() {
                for (i, group) in groups.iter().enumerate() {
                    let name = group["displayName"]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("Group {}", i + 1));
                    if let Some(buckets) = group["buckets"].as_array() {
                        for kind in ["5h", "weekly"] {
                            for (j, bucket) in buckets.iter().enumerate() {
                                if bucket["window"].as_str() != Some(kind) {
                                    continue;
                                }
                                if let Some(remaining) = bucket["remainingFraction"]
                                    .as_f64()
                                    .filter(|n| n.is_finite())
                                {
                                    result.push(UsageWindow {
                                        id: format!("{i}-{kind}-{j}"),
                                        group: Some(name.clone()),
                                        label: if kind == "5h" { "5시간" } else { "주간" }.into(),
                                        used: (1.0 - remaining) * 100.0,
                                        reset_at: date(&bucket["resetTime"]),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if result.is_empty() {
        Err("응답에서 사용량 정보를 찾지 못했습니다.".into())
    } else {
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn claude_fractional_reset() {
        let windows = parse(&json!({"five_hour":{"utilization":24.5,"resets_at":"2026-10-14T08:00:00.123Z"},"seven_day":{"utilization":63}}), Provider::Claude).unwrap();
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].used, 24.5);
        assert_eq!(windows[0].reset_at.unwrap().timestamp_subsec_millis(), 123);
        assert!(windows[1].reset_at.is_none());
    }
    #[test]
    fn codex_epoch_and_missing_weekly() {
        let windows = parse(&json!({"rate_limit":{"primary_window":{"used_percent":37,"reset_at":1791964800},"secondary_window":null}}), Provider::Codex).unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].reset_at.unwrap().timestamp(), 1791964800);
    }
    #[test]
    fn antigravity_order_and_maximum() {
        let windows = parse(&json!({"groups":[{"displayName":"Gemini","buckets":[{"window":"weekly","remainingFraction":0.2},{"window":"5h","remainingFraction":0.75},{"window":"other","remainingFraction":0}]},{"displayName":"Claude","buckets":[{"window":"5h","remainingFraction":0.1}]}]}), Provider::Antigravity).unwrap();
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0].used, 25.0);
        assert_eq!(
            Snapshot {
                windows,
                ..Default::default()
            }
            .headline(),
            Some(90.0)
        );
    }
    #[test]
    fn invalid_is_not_zero() {
        for p in Provider::ALL {
            assert!(parse(&json!({}), p).is_err());
            assert!(parse(&json!([]), p).is_err());
        }
    }
    #[test]
    fn clamp_weekly_and_stale_title() {
        let mut states: BTreeMap<_, _> = Provider::ALL
            .into_iter()
            .map(|p| (p, Snapshot::default()))
            .collect();
        assert_eq!(title(&states, "all"), "C —  X —  A —");
        let codex = states.get_mut(&Provider::Codex).unwrap();
        codex.apply(Ok(vec![UsageWindow {
            id: "w".into(),
            group: None,
            label: "주간".into(),
            used: 150.0,
            reset_at: None,
        }]));
        codex.apply(Err("만료".into()));
        assert_eq!(codex.headline(), Some(100.0));
        assert_eq!(title(&states, "codex"), "X 100%*");
        states.get_mut(&Provider::Codex).unwrap().apply(Ok(vec![]));
        assert!(states[&Provider::Codex].error.is_none());
        assert!(states[&Provider::Claude].windows.is_empty());
    }
}
