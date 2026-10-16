use crate::{
    credentials,
    usage::{self, Provider, UsageWindow},
};
use reqwest::{Client, RequestBuilder, StatusCode};
use serde_json::{json, Value};
use std::time::Duration;

pub fn client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "HTTP 클라이언트를 초기화하지 못했습니다.".into())
}
pub fn http_error(status: StatusCode) -> String {
    let hint = match status {
        StatusCode::UNAUTHORIZED => "인증이 만료되었습니다. 해당 CLI에서 다시 로그인해 주세요.",
        StatusCode::FORBIDDEN => "이 계정으로 사용량에 접근할 수 없습니다.",
        StatusCode::TOO_MANY_REQUESTS => "요청이 많습니다. 잠시 후 다시 시도해 주세요.",
        _ => "사용량 조회에 실패했습니다.",
    };
    format!("HTTP {}: {}", status.as_u16(), hint)
}
async fn request(request: RequestBuilder) -> Result<Value, String> {
    let response = request.send().await.map_err(|e| {
        if e.is_timeout() {
            "조회 시간이 초과되었습니다."
        } else {
            "서버에 연결할 수 없습니다. 네트워크를 확인해 주세요."
        }
    })?;
    if !response.status().is_success() {
        return Err(http_error(response.status()));
    }
    // Do not put tokens, raw errors, response bodies or credential paths in UI events/logs.
    response
        .json()
        .await
        .map_err(|_| "사용량 응답 형식을 확인할 수 없습니다.".into())
}
fn text<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value[field].as_str().filter(|s| !s.trim().is_empty())
}

pub async fn fetch(client: &Client, provider: Provider) -> Result<Vec<UsageWindow>, String> {
    if provider == Provider::Antigravity {
        let mut errors = vec![];
        for account in ["antigravity", "antigravity-acp"] {
            match antigravity(client, account).await {
                Ok(windows) => return Ok(windows),
                Err(error) => errors.push(format!("{account}: {error}")),
            }
        }
        return Err(errors.join("\n"));
    }
    let credential = credentials::read(provider, None).await?;
    let codex = provider == Provider::Codex;
    let oauth = &credential[if codex { "tokens" } else { "claudeAiOauth" }];
    let token = text(oauth, if codex { "access_token" } else { "accessToken" })
        .ok_or_else(|| format!("{} CLI에서 먼저 로그인해 주세요.", provider.name()))?;
    let builder = if codex {
        let account = text(oauth, "account_id")
            .ok_or("Codex CLI에서 ChatGPT 계정으로 다시 로그인해 주세요.")?;
        client
            .get("https://chatgpt.com/backend-api/wham/usage")
            .bearer_auth(token)
            .header("chatgpt-account-id", account)
    } else {
        client
            .get("https://api.anthropic.com/api/oauth/usage")
            .bearer_auth(token)
            .header("anthropic-beta", "oauth-2025-04-20")
    };
    usage::parse(&request(builder).await?, provider)
}
async fn antigravity(client: &Client, account: &str) -> Result<Vec<UsageWindow>, String> {
    let credential = credentials::read(Provider::Antigravity, Some(account)).await?;
    let token = credential
        .get("token")
        .filter(|t| t.is_object())
        .unwrap_or(&credential);
    let mut access = text(token, "access_token").map(str::to_owned);
    let expiry = usage::date(&token["expiry"]).or_else(|| usage::date(&token["expires_at"]));
    if access.is_none()
        || expiry.is_some_and(|e| e <= chrono::Utc::now() + chrono::Duration::seconds(60))
    {
        let refresh = text(token, "refresh_token")
            .ok_or("토큰이 만료되었습니다. agy models 실행 후 새로고침해 주세요.")?;
        let client_id = text(token, "client_id")
            .or_else(|| text(&credential, "client_id"))
            .ok_or("토큰을 갱신할 수 없습니다. agy models 실행 후 새로고침해 주세요.")?;
        let secret = text(token, "client_secret")
            .or_else(|| text(&credential, "client_secret"))
            .unwrap_or("");
        let renewed = request(client.post("https://oauth2.googleapis.com/token").json(&json!({"client_id":client_id,"client_secret":secret,"refresh_token":refresh,"grant_type":"refresh_token"}))).await?;
        access = text(&renewed, "access_token").map(str::to_owned);
    }
    let access = access.ok_or("Antigravity access_token을 찾지 못했습니다.")?;
    let mut errors = vec![];
    for host in [
        "daily-cloudcode-pa.googleapis.com",
        "cloudcode-pa.googleapis.com",
    ] {
        let response = request(client.post(format!("https://{host}/v1internal:retrieveUserQuotaSummary"))
            .bearer_auth(&access).header("User-Agent", format!("antigravity/cli/1.3.6 (aidev_client; os_type={}; arch={}; cl=999748552; auth_method=consumer)", if cfg!(target_os = "macos") { "darwin" } else { std::env::consts::OS }, std::env::consts::ARCH)).json(&json!({}))).await;
        match response.and_then(|json| usage::parse(&json, Provider::Antigravity)) {
            Ok(windows) => return Ok(windows),
            Err(error) => errors.push(error),
        }
    }
    Err(errors.join(" / "))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_hints() {
        assert!(http_error(StatusCode::UNAUTHORIZED).contains("인증이 만료"));
        assert!(http_error(StatusCode::FORBIDDEN).contains("접근"));
        assert!(http_error(StatusCode::TOO_MANY_REQUESTS).contains("요청이 많"));
    }
}
