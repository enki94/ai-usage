use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub interval: u64,
    pub selection: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            interval: 0,
            selection: "all".into(),
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if ![0, 600, 3600].contains(&self.interval) {
            return Err("자동 새로고침 간격이 올바르지 않습니다.".into());
        }
        if !["all", "claude", "codex", "antigravity"].contains(&self.selection.as_str()) {
            return Err("표시 서비스가 올바르지 않습니다.".into());
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Self>(&b).ok())
            .filter(|s| s.validate().is_ok())
            .unwrap_or_default()
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let parent = path.parent().ok_or("설정 경로가 올바르지 않습니다.")?;
        std::fs::create_dir_all(parent).map_err(|_| "설정 폴더를 만들지 못했습니다.")?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|_| "설정을 저장하지 못했습니다.")?;
        let temporary = path.with_extension("tmp");
        std::fs::write(&temporary, bytes).map_err(|_| "설정을 저장하지 못했습니다.")?;
        std::fs::rename(temporary, path).map_err(|_| "설정을 저장하지 못했습니다.".into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_settings_rejected() {
        assert!(Settings {
            interval: 30,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(Settings {
            selection: "invalid".into(),
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(Settings::default().validate().is_ok());
    }
}
