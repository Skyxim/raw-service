use std::{fmt::Display, str::FromStr};

use gitlab::gitlab::Repos;

use worker::{js_sys::RegExp, *};
mod gitlab;
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

#[derive(Debug, PartialEq, Eq)]
enum BackendType {
    Github(String),
    Gitlab(String),
    Bitbucket(String),
}

impl BackendType {
    pub async fn parse_url(&self, path: &str, env: &Env) -> std::result::Result<Url, String> {
        match self {
            BackendType::Bitbucket(url) | BackendType::Github(url) => {
                return Url::from_str(&format!("{}/{}", url, path)).map_err(|op| op.to_string());
            }
            BackendType::Gitlab(url) => {
                // 处理 username/repository/raw/branch/filepath
                // username/repository/raw/-/branch/filepath
                // 到 /api/v4/projects/${project_id}/repository/files/${filepath}/raw?ref=${branch}
                // 其中 project_id 使用 username 和 repository 通过 https://gitlab.com/api/v4/projects?owned=true&simple=true 获取
                let reg = RegExp::new("/(.+?/.+?)(/-)?/raw/(.+?)/(.+)", "g");
                if let Some(arr) = reg.exec(path) {
                    let username_and_repo = arr
                        .get(1)
                        .as_string()
                        .ok_or(String::from("not found username and repo"))?
                        .to_lowercase();
                    let filepath = arr
                        .get(arr.length()-1)
                        .as_string()
                        .ok_or(String::from("not found file path"))?;
                    let branch = arr
                        .get(arr.length()-2)
                        .as_string()
                        .ok_or(String::from("not found branch"))?;
                    let mut repo = Repos::new(env)?;
                    let id = repo.get_id(&username_and_repo).await?;
                    return Url::from_str(&format!(
                        "{}/api/v4/projects/{}/repository/files/{}/raw?ref={}",
                        url, id, filepath, branch
                    ))
                    .map_err(|op| op.to_string());
                }

                return Err("TODO".to_owned());
            }
        }
    }
}

impl Display for BackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendType::Bitbucket(url) => f.write_str(url),
            BackendType::Github(url) => f.write_str(url),
            BackendType::Gitlab(url) => f.write_str(url),
        }
    }
}
#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: worker::Context) -> Result<Response> {
    log_request(&req);
    let backend_type = if let Ok(backend) = env.var("BACKEND") {
        parse_backend_type(&backend.to_string())
    } else {
        parse_backend_type("")
    };
    // Optionally, get more helpful error messages written to the console in the case of a panic.
    utils::set_panic_hook();
    if req.method() != Method::Get {
        return Response::error("Method Not Allowed", 405);
    }

    let path = req.path();
    let mut is_verify = false;
    if let Ok(token) = env.secret("TOKEN") {
        let url = req.url()?;
        let ok = url
            .query_pairs()
            .find(|param| param.0.to_lowercase() == "token" && param.1 == token.to_string())
            .is_some();
        is_verify = ok;
    }

    if is_verify {
        let u = backend_type.parse_url(&path, &env).await?;
        console_log!("access url: {}", u);
        let client = reqwest::Client::new();
        let mut req = client.get(u);
        console_log!("token is verfied, add auth");
        match backend_type {
            BackendType::Github(_) => {
                let token = env.secret("GITHUB_TOKEN")?.to_string();
                console_log!("use github");
                req = req.bearer_auth(token);
            }
            BackendType::Bitbucket(_) => {
                let username = env.secret("BITBUCKET_USERNAME")?.to_string();
                let password = env.secret("BITBUCKET_PASSWORD")?.to_string();
                console_log!("use bitbucket");
                req = req.basic_auth(username.to_string(), Some(password.to_string()));
            }
            BackendType::Gitlab(_) => {
                let token = env.secret("GITLAB_TOKEN")?.to_string();
                req = req.bearer_auth(token);
            }
        }

        let resp = req.send().await.map_err(|e| e.to_string())?;
        if let Ok(text) = resp.text().await {
            return Response::ok(text);
        } else {
            return Response::error("Github access failed", 500);
        }
    } else {
        console_log!("Not found token")
    }
    return Response::error("Invaild path", 400);
}

fn parse_backend_type(b_type: &str) -> BackendType {
    match b_type.to_uppercase().as_str() {
        "GITHUB" => BackendType::Github(String::from("https://raw.githubusercontent.com")),
        "BITBUCKET" => BackendType::Bitbucket(String::from("https://api.bitbucket.org")),
        _ => BackendType::Gitlab(String::from("https://gitlab.com")),
    }
}
