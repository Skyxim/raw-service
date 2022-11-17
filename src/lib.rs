use std::str::FromStr;

use reqwest::header::{HeaderMap, HeaderValue};
use worker::*;

mod utils;
fn log_request(req: &Request) {
    console_log!(
        "{} - [{}], located at: {:?}, within: {}",
        Date::now().to_string(),
        req.path(),
        req.cf().coordinates().unwrap_or_default(),
        req.cf().region().unwrap_or("unknown region".into())
    );
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: worker::Context) -> Result<Response> {
    log_request(&req);
    let mut BASE_URL = "https://raw.githubusercontent.com".to_string();

    if let Ok(base_url) = env.var("BASE_URL") {
        BASE_URL = base_url.to_string();
    }
    // Optionally, get more helpful error messages written to the console in the case of a panic.
    utils::set_panic_hook();
    let router = Router::new();
    if req.method() != Method::Get {
        return Response::error("Method Not Allowed", 405);
    }

    let path = req.path();
    let mut isVerify = false;
    if let Ok(token) = env.secret("TOKEN") {
        let url = req.url()?;
        let ok = url
            .query_pairs()
            .find(|param| param.0.to_lowercase() == "token" && param.1 == token.to_string())
            .is_some();
        isVerify = ok;
    }

    let newUrl = format!("{}/{}", BASE_URL, path);
    console_log!("access url: {}", newUrl);
    if let Ok(u) = Url::from_str(&newUrl) {
        let client = reqwest::Client::new();
        let mut req = client.get(u);
        if isVerify {
            console_log!("token is verfied, add auth");
            if let Ok(username) = env.secret("BITBUCKET_USERNAME") {
                if let Ok(password) = env.secret("BITBUCKET_PASSWORD") {
                    console_log!("use bitbucket");
                    req = req.basic_auth(username.to_string(), Some(password.to_string()));
                }
            } else {
                if let Ok(githubToken) = env.secret("GITHUB_TOKEN") {
                    console_log!("use github");
                    req = req.bearer_auth(githubToken.to_string());
                }
            }
        } else {
            console_log!("no token");
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        if let Ok(text) = resp.text().await {
            return Response::ok(text);
        } else {
            return Response::error("Github access failed", 500);
        }
    }
    return Response::error("Invaild path", 400);
}
