use crate::usage::Provider;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;

fn decode(blob: &[u8]) -> Result<String, String> {
    let text = if blob.len() > 1 && blob[1] == 0 {
        if blob.len() % 2 != 0 {
            return Err("자격 증명 인코딩이 올바르지 않습니다.".into());
        }
        let utf16: Vec<u16> = blob
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        String::from_utf16(&utf16).map_err(|_| "자격 증명 인코딩이 올바르지 않습니다.")?
    } else {
        String::from_utf8(blob.to_vec()).map_err(|_| "자격 증명 인코딩이 올바르지 않습니다.")?
    };
    let text = text.trim().trim_end_matches('\0');
    if let Some(encoded) = text.strip_prefix("go-keyring-base64:") {
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| "자격 증명 인코딩이 올바르지 않습니다.")?;
        String::from_utf8(bytes).map_err(|_| "자격 증명 인코딩이 올바르지 않습니다.".into())
    } else {
        Ok(text.to_owned())
    }
}
#[cfg(windows)]
fn read_generic(target: &str) -> Result<Option<String>, String> {
    use std::{ffi::c_void, ptr};
    #[repr(C)]
    struct Credential {
        flags: u32,
        kind: u32,
        target_name: *mut u16,
        comment: *mut u16,
        last_written: [u32; 2],
        blob_size: u32,
        blob: *mut u8,
        persist: u32,
        attribute_count: u32,
        attributes: *mut c_void,
        target_alias: *mut u16,
        user_name: *mut u16,
    }
    #[link(name = "Advapi32")]
    extern "system" {
        fn CredReadW(
            target: *const u16,
            kind: u32,
            flags: u32,
            credential: *mut *mut Credential,
        ) -> i32;
        fn CredFree(buffer: *mut c_void);
    }
    #[link(name = "Kernel32")]
    extern "system" {
        fn GetLastError() -> u32;
    }
    let name: Vec<u16> = target.encode_utf16().chain(Some(0)).collect();
    let mut credential = ptr::null_mut();
    // CredReadW allocates one owned buffer; copy/decode before CredFree. Nothing crosses an await.
    unsafe {
        if CredReadW(name.as_ptr(), 1, 0, &mut credential) == 0 {
            return if GetLastError() == 1168 {
                Ok(None)
            } else {
                Err("Windows 자격 증명 저장소를 읽지 못했습니다.".into())
            };
        }
        if credential.is_null() {
            return Err("자격 증명 저장소 응답이 올바르지 않습니다.".into());
        }
        let c = &*credential;
        let result = if c.blob_size == 0 || c.blob.is_null() {
            Err("자격 증명이 비어 있습니다.".into())
        } else {
            decode(std::slice::from_raw_parts(c.blob, c.blob_size as usize)).map(Some)
        };
        CredFree(credential.cast());
        result
    }
}

// Capture stdout in memory only; never expose keychain output or stderr to the UI.
#[cfg(any(target_os = "macos", target_os = "linux", test))]
async fn read_secret(
    program: &str,
    args: &[&str],
    missing_code: i32,
) -> Result<Option<String>, String> {
    use std::{process::Stdio, time::Duration};
    let child = tokio::process::Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| "인증 저장소 도구를 실행하지 못했습니다. macOS 키체인 또는 Linux secret-tool 설치를 확인해 주세요.")?;
    let output = tokio::time::timeout(Duration::from_secs(20), child.wait_with_output())
        .await
        .map_err(|_| {
            "인증 저장소 응답 시간이 초과되었습니다. 키체인 잠금 및 접근 권한을 확인해 주세요."
        })?
        .map_err(|_| "인증 저장소를 읽지 못했습니다.")?;
    if output.status.success() {
        let value = decode(&output.stdout)?;
        return Ok((!value.is_empty()).then_some(value));
    }
    if output.status.code() == Some(missing_code) {
        Ok(None)
    } else {
        Err(
            "인증 저장소를 읽지 못했습니다. 해당 CLI 로그인과 키체인 접근 권한을 확인해 주세요."
                .into(),
        )
    }
}

async fn antigravity_secret(account: &str) -> Result<Option<String>, String> {
    #[cfg(windows)]
    {
        let target = format!("gemini:{account}");
        tokio::task::spawn_blocking(move || read_generic(&target))
            .await
            .map_err(|_| "자격 증명을 읽지 못했습니다.")?
    }
    #[cfg(target_os = "macos")]
    {
        read_secret(
            "/usr/bin/security",
            &["find-generic-password", "-s", "gemini", "-a", account, "-w"],
            44,
        )
        .await
    }
    #[cfg(target_os = "linux")]
    {
        read_secret(
            "secret-tool",
            &["lookup", "service", "gemini", "username", account],
            1,
        )
        .await
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = account;
        Ok(None)
    }
}

fn config_path(
    provider: Provider,
    home: std::path::PathBuf,
    override_dir: Option<std::ffi::OsString>,
) -> std::path::PathBuf {
    let (directory, file) = if provider == Provider::Claude {
        (".claude", ".credentials.json")
    } else {
        (".codex", "auth.json")
    };
    override_dir
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(directory))
        .join(file)
}

pub async fn read(provider: Provider, account: Option<&str>) -> Result<Value, String> {
    let text = if provider == Provider::Antigravity {
        // An explicit file also works on machines without a desktop secret service.
        if let Some(path) = std::env::var_os("AI_USAGE_ANTIGRAVITY_AUTH").filter(|v| !v.is_empty())
        {
            tokio::fs::read_to_string(path)
                .await
                .map_err(|_| "Antigravity 인증 파일을 읽지 못했습니다.")?
        } else {
            antigravity_secret(account.unwrap_or("antigravity")).await?
                .ok_or("Antigravity 인증을 찾지 못했습니다. agy 로그인 또는 AI_USAGE_ANTIGRAVITY_AUTH를 확인해 주세요.")?
        }
    } else {
        let home = dirs::home_dir().ok_or("사용자 홈 폴더를 찾지 못했습니다.")?;
        let env = if provider == Provider::Claude {
            "CLAUDE_CONFIG_DIR"
        } else {
            "CODEX_HOME"
        };
        let override_dir = std::env::var_os(env).filter(|s| !s.is_empty());
        #[cfg(target_os = "macos")]
        if provider == Provider::Claude && override_dir.is_none() {
            if let Some(text) = read_secret(
                "/usr/bin/security",
                &[
                    "find-generic-password",
                    "-s",
                    "Claude Code-credentials",
                    "-w",
                ],
                44,
            )
            .await?
            {
                return parse_json(&text);
            }
        }
        tokio::fs::read_to_string(config_path(provider, home, override_dir)).await
            .map_err(|_| format!("{} CLI에서 먼저 로그인해 주세요. 사용자 홈 폴더의 인증 파일을 읽지 못했습니다.", provider.name()))?
    };
    parse_json(&text)
}

fn parse_json(text: &str) -> Result<Value, String> {
    serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|_| "CLI 인증 JSON 형식이 올바르지 않습니다.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encodings() {
        assert_eq!(decode(br#"{"token":1}"#).unwrap(), r#"{"token":1}"#);
        assert_eq!(
            decode(b"go-keyring-base64:eyJ0b2tlbiI6MX0=").unwrap(),
            r#"{"token":1}"#
        );
        let utf16: Vec<u8> = r#"{"token":1}"#.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(decode(&utf16).unwrap(), r#"{"token":1}"#);
        assert!(decode(b"go-keyring-base64:invalid!").is_err());
    }
    #[test]
    fn credential_paths_respect_overrides() {
        let home = std::path::PathBuf::from("home");
        assert_eq!(
            config_path(Provider::Claude, home.clone(), None),
            home.join(".claude/.credentials.json")
        );
        assert_eq!(
            config_path(Provider::Codex, home.clone(), Some("custom".into())),
            std::path::PathBuf::from("custom/auth.json")
        );
        assert_eq!(
            config_path(Provider::Codex, home.clone(), Some("".into())),
            home.join(".codex/auth.json")
        );
    }
    #[test]
    fn invalid_credentials_do_not_leak_contents() {
        assert!(parse_json("\u{feff}{\"tokens\":{}}").is_ok());
        let error = parse_json("secret-token-invalid-json").unwrap_err();
        assert!(!error.contains("secret-token"));
    }
    #[tokio::test(flavor = "current_thread")]
    async fn secret_command_captures_and_decodes_stdout() {
        #[cfg(windows)]
        let (program, args) = (
            "cmd.exe",
            vec!["/d", "/c", "echo go-keyring-base64:eyJ0b2tlbiI6MX0="],
        );
        #[cfg(not(windows))]
        let (program, args) = (
            "/bin/sh",
            vec!["-c", "printf '%s\\n' go-keyring-base64:eyJ0b2tlbiI6MX0="],
        );
        assert_eq!(
            read_secret(program, &args, 44).await.unwrap(),
            Some(r#"{"token":1}"#.into())
        );
    }
    #[tokio::test(flavor = "current_thread")]
    async fn secret_command_distinguishes_missing_and_failure_without_leaking_stderr() {
        #[cfg(windows)]
        let (program, args) = (
            "cmd.exe",
            vec!["/d", "/c", "echo private-fixture 1>&2 & exit /b 7"],
        );
        #[cfg(not(windows))]
        let (program, args) = ("/bin/sh", vec!["-c", "echo private-fixture >&2; exit 7"]);
        assert_eq!(read_secret(program, &args, 7).await.unwrap(), None);
        let error = read_secret(program, &args, 44).await.unwrap_err();
        assert!(!error.contains("private-fixture"));
    }
}
