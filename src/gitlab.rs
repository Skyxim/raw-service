pub mod gitlab {
    use std::{
        collections::{hash_map, HashMap},
        f32::consts::E,
        fs::Metadata,
        str::FromStr,
        string, vec,
    };

    use reqwest::{Method, Url};
    use serde::{Deserialize, Serialize};
    use worker::{console_debug, console_log, worker_sys::cache, Env};

    use crate::_worker_fetch::fetch;

    #[derive(Serialize, Deserialize)]
    struct RepoInfo {
        id: i32,
        path: String,
        path_with_namespace: String,
        default_branch: String,
    }

    const CACHE_KEY: &'static str = "REPO_LIST_KEY";
    const NAMESPACE: &'static str = "RAW_SERVICE_KV";
    pub struct Repos<'a> {
        env: &'a Env,
    }

    impl<'a> Repos<'a> {
        pub fn new(env: &'a Env) -> Result<Repos<'a>, String> {
            return Ok(Repos { env: env });
        }

        pub async fn get_id(&mut self, username_and_repo: &str) -> Result<i32, String> {
            console_debug!("find: {}", username_and_repo);
            match Self::fetch_from_cache(self.env).await {
                Ok(list) => {
                    if let Some(id) = list.get(username_and_repo) {
                        console_debug!("use cache");
                        return Ok(id.id);
                    }
                }
                Err(_) => {}
            }

            console_debug!("fetch new infos");
            let repo_list = self.fetch_resp().await?;
            let map = Self::map_repo_info(repo_list);
            match self.cache(&map).await {
                Ok(_) => {}
                Err(e) => {
                    console_debug!("err: {}", e);
                }
            }
            return map
                .get(username_and_repo)
                .map(|i| i.id)
                .ok_or(String::from("Not found"));
        }

        async fn fetch_from_cache(env: &Env) -> Result<HashMap<String, RepoInfo>, String> {
            let kv = env.kv(NAMESPACE).map_err(|e| e.to_string())?;
            return match kv.get(CACHE_KEY).text().await {
                Ok(kv) => {
                    let list: HashMap<String, RepoInfo> =
                        serde_json::from_str(kv.ok_or("not have cache")?.as_str()).unwrap();
                    Ok(list)
                }
                Err(e) => Err(e.to_string()),
            };
        }

        async fn fetch_resp(&self) -> Result<Vec<RepoInfo>, String> {
            let token = self
                .env
                .secret("GITLAB_TOKEN")
                .map_err(|e| e.to_string())?
                .to_string();
            let u = Url::from_str("https://gitlab.com/api/v4/projects?owned=true&simple=true")
                .map_err(|e| e.to_string())?;
            let client = reqwest::Client::new();
            let resp = client
                .get(u)
                .bearer_auth(token)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let result = resp
                .json::<Vec<RepoInfo>>()
                .await
                .map_err(|e| e.to_string())?;
            return Ok(result);
        }

        fn map_repo_info(list: Vec<RepoInfo>) -> HashMap<String, RepoInfo> {
            return list
                .into_iter()
                .map(|e| (e.path_with_namespace.to_string().to_lowercase(), e))
                .collect::<HashMap<String, RepoInfo>>();
        }

        async fn cache(&mut self, map: &HashMap<String, RepoInfo>) -> Result<(), String> {
            let kv = self.env.kv(NAMESPACE).map_err(|e| e.to_string())?;
            return match kv.put(CACHE_KEY, map) {
                Ok(r) => {
                    console_debug!("kv put");
                    r.execute().await.map_err(|e| e.to_string())
                }
                Err(e) => Err(e.to_string()),
            };
        }
    }
}
