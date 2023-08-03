use super::version::{parse_html, Version};
use futures::stream::{Stream, StreamExt};
use reqwest::{header, StatusCode, Url};
use sqlx::{
    query, query_as, query_scalar, sqlite::SqlitePool, types::time::OffsetDateTime,
    Error as SqlxError, FromRow,
};
use std::{fmt, iter::Iterator, pin::Pin, str::FromStr, time::Duration};

#[derive(Deserialize)]
struct PypiProject {
    info: PypiProjectInfo,
}

#[derive(Deserialize)]
struct PypiProjectInfo {
    version: String,
}

#[derive(Deserialize)]
struct GitHubReleaseInfo {
    tag_name: String,
}

#[derive(FromRow)]
pub struct Package {
    id: i64,
    distname: String,
    master_site: String,
    version: String,
    local_version: Option<String>,
    last_check: OffsetDateTime,
}

impl Package {
    pub async fn add(
        pool: &SqlitePool,
        distname: String,
        master_site: String,
        version: String,
    ) -> Result<Self, SqlxError> {
        let last_check = OffsetDateTime::now_utc();
        query_as!(
            Self,
            "INSERT INTO package (distname, master_site, version, local_version, last_check) \
            VALUES ($1, $2, $3, $4, $5) RETURNING *",
            distname,
            master_site,
            version,
            version,
            last_check
        )
        .fetch_one(pool)
        .await
    }

    pub async fn update(
        &mut self,
        pool: &SqlitePool,
        distname: Option<String>,
        master_site: Option<String>,
        version: Option<String>,
    ) -> Result<(), SqlxError> {
        let mut run_query = false;

        if let Some(distname) = distname {
            self.distname = distname;
            run_query = true;
        }
        if let Some(master_site) = master_site {
            self.master_site = master_site;
            run_query = true;
        }
        if let Some(version) = version {
            self.local_version = Some(version);
            run_query = true;
        }

        if run_query {
            query_as!(
                Self,
                "UPDATE package SET distname = $2, master_site = $3, local_version = $4 \
                WHERE id = $1",
                self.id,
                self.distname,
                self.master_site,
                self.local_version,
            )
            .execute(pool)
            .await?;
        }

        Ok(())
    }

    pub async fn fetch_by_name(pool: &SqlitePool, name: &str) -> Result<Self, SqlxError> {
        query_as!(
            Self,
            "SELECT id, distname \"distname!\", master_site \"master_site!\", version \"version!\", local_version, last_check \
            FROM package WHERE distname = $1",
            name
        ).fetch_one(pool).await
    }

    pub async fn all_from_db(pool: &SqlitePool) -> Result<Vec<Self>, SqlxError> {
        let pkgs = query_as!(
            Self,
            "SELECT id, distname \"distname!\", master_site \"master_site!\", version \"version!\", local_version, last_check \
            FROM package ORDER BY distname",
        ).fetch_all(pool).await?;

        for pkg in &pkgs {
            println!("{pkg}");
        }

        Ok(pkgs)
    }

    pub async fn total(pool: &SqlitePool) -> Result<i32, SqlxError> {
        query_scalar!("SELECT count(*) FROM package WHERE local_version != version")
            .fetch_one(pool)
            .await
    }

    /// Build asynchronous stream to fetch all packages.
    fn timed_stream(
        pool: &SqlitePool,
    ) -> Pin<Box<dyn Stream<Item = Result<Self, SqlxError>> + Send + '_>> {
        let two_hours_ago = OffsetDateTime::now_utc() - Duration::from_secs(7200);
        // macro error: cannot return value referencing local variable `two_hours_ago`
        query_as(
            "SELECT id, distname, master_site, version, local_version, last_check \
            FROM package WHERE last_check <= $1 ORDER BY distname",
        )
        .bind(two_hours_ago)
        .fetch(pool)
    }

    /// Build asynchronous stream to fetch all packages.
    fn stream(
        pool: &SqlitePool,
    ) -> Pin<Box<dyn Stream<Item = Result<Self, SqlxError>> + Send + '_>> {
        query_as!(
            Self,
            "SELECT id, distname \"distname!\", master_site \"master_site!\", version \"version!\", local_version, last_check \
            FROM package ORDER BY distname"
        ).fetch(pool)
    }

    /// Switch to newer PyPI URL.
    async fn fix_pypi(&mut self, pool: &SqlitePool) -> Result<bool, SqlxError> {
        let mut update = false;

        if self.master_site.ends_with('/') {
            update = true;
            self.master_site.pop();
        }

        if self.master_site.contains("pypi.python.org/pypi/") {
            update = true;
            self.master_site = self
                .master_site
                .replace("pypi.python.org/pypi/", "pypi.org/project/");
        }

        if update {
            query!(
                "UPDATE package SET master_site = $2 WHERE id = $1",
                self.id,
                self.master_site
            )
            .execute(pool)
            .await?;
        }

        Ok(update)
    }

    /// Store version and last check
    pub async fn store_version(&mut self, pool: &SqlitePool) -> Result<(), SqlxError> {
        self.last_check = OffsetDateTime::now_utc();

        query!(
            "UPDATE package SET version = $2, last_check = $3 WHERE id = $1",
            self.id,
            self.version,
            self.last_check,
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Mark as latest (verion and local version are the same).
    pub async fn mark_latest(&mut self, pool: &SqlitePool) -> Result<(), SqlxError> {
        if let Some(local_version) = &self.local_version {
            if local_version == &self.version {
                println!(
                    "Package {} already has latest version {}",
                    self.distname, local_version
                );
                return Ok(());
            }
        }

        query!(
            "UPDATE package SET local_version = $2, last_check = $3 WHERE id = $1",
            self.id,
            self.version,
            self.last_check,
        )
        .execute(pool)
        .await?;

        if let Some(local_version) = &self.local_version {
            println!(
                "Package {} updated from {} to {}",
                self.distname, local_version, self.version
            );
        } else {
            println!("Package {} version set to {}", self.distname, self.version);
        }
        self.local_version = Some(self.version.clone());

        Ok(())
    }

    // pub async fn set_local_version(
    //     &mut self,
    //     pool: &SqlitePool,
    //     local_version: String,
    // ) -> Result<(), SqlxError> {
    //     query!(
    //         "UPDATE package SET local_version = $2, last_check = $3 WHERE id = $1",
    //         self.id,
    //         local_version,
    //         self.last_check,
    //     )
    //     .execute(pool)
    //     .await?;

    //     self.local_version = Some(local_version);

    //     Ok(())
    // }

    /// Update last check
    pub async fn update_last_check(&mut self, pool: &SqlitePool) -> Result<(), SqlxError> {
        self.last_check = OffsetDateTime::now_utc();

        query!(
            "UPDATE package SET last_check = $3 WHERE id = $1",
            self.id,
            self.last_check,
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Forget about this package
    pub async fn delete(self, pool: &SqlitePool) -> Result<(), SqlxError> {
        query!("DELETE FROM package WHERE id = $1", self.id,)
            .execute(pool)
            .await?;

        println!("Removed {}", self.distname);

        Ok(())
    }

    /// Use while
    pub async fn stream_from_db(pool: &SqlitePool) {
        while let Some(pkg) = Self::stream(pool).next().await {
            if let Ok(pkg) = pkg {
                if !pkg.is_latest() {
                    println!("{pkg}");
                }
            }
        }
    }

    /// Use for_each_concurrent()
    pub async fn info_stream(pool: &SqlitePool) {
        Self::stream(pool)
            .for_each_concurrent(10, |pkg| async move {
                if let Ok(pkg) = pkg {
                    if !pkg.is_latest() {
                        println!("{pkg}");
                    }
                }
            })
            .await;
    }

    pub async fn check_all(
        pool: &SqlitePool,
        github_account: Option<&String>,
        github_token: Option<&String>,
    ) {
        Self::timed_stream(pool)
            .for_each_concurrent(10, |pkg| async move {
                if let Ok(mut pkg) = pkg {
                    pkg.fix_pypi(pool).await.unwrap();
                    if pkg.auto_check(github_account, github_token).await {
                        pkg.store_version(pool).await.unwrap();
                    } else {
                        pkg.update_last_check(pool).await.unwrap();
                    }
                }
            })
            .await;
    }

    fn parse_pypi(&mut self, pypi_project: PypiProject) -> bool {
        if self.version == pypi_project.info.version {
            false
        } else {
            println!(
                "{} {} -> {}",
                self.distname,
                self.local_version.as_deref().unwrap_or("-"),
                pypi_project.info.version
            );
            self.version = pypi_project.info.version;
            true
        }
    }

    fn parse_github(&mut self, github_info: &GitHubReleaseInfo) -> bool {
        let version = github_info
            .tag_name
            .trim_start_matches(|c| !char::is_ascii_digit(&c));
        if self.version != version {
            println!(
                "{} {} -> {}",
                self.distname,
                self.local_version.as_deref().unwrap_or("-"),
                version
            );
            self.version = version.into();
            true
        } else {
            false
        }
    }

    pub async fn auto_check(
        &mut self,
        github_account: Option<&String>,
        github_token: Option<&String>,
    ) -> bool {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static("Version-Tracker"),
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();

        let url = Url::parse(&self.master_site).unwrap();
        if let Some(hostname) = url.domain() {
            match hostname {
                "pypi.org" => {
                    if let Some(project) = url.path_segments().and_then(Iterator::last) {
                        let response = client
                            .get(format!("https://pypi.org/pypi/{project}/json"))
                            .send()
                            .await
                            .unwrap();
                        if response.status() != StatusCode::OK {
                            eprintln!("Status {}", response.status());
                            // eprintln!("Text: {:?}", response.text().await.unwrap());
                            return false;
                        }
                        match response.json::<PypiProject>().await {
                            Ok(pypi_project) => {
                                return self.parse_pypi(pypi_project);
                            }
                            Err(err) => {
                                println!(
                                    "JSON error for {} [{}]: {}",
                                    self.distname, self.master_site, err
                                );
                            }
                        }
                    }
                }
                // https://docs.github.com/en/rest/releases/releases#get-the-latest-release
                // TODO: Accept: application/vnd.github.v3+json
                "github.com" => {
                    let path =
                        format!("https://api.github.com/repos{}/releases/latest", url.path());
                    let mut request = client.get(path);
                    if let Some(account) = github_account {
                        // Token (classic) with "read:project" access
                        request = request.basic_auth(account, github_token);
                    }
                    let response = request.send().await.unwrap();
                    if response.status() != StatusCode::OK {
                        eprintln!("Status {}", response.status());
                        // eprintln!("Text: {:?}", response.text().await.unwrap());
                        return false;
                    }
                    match response.json::<GitHubReleaseInfo>().await {
                        Ok(github_info) => {
                            return self.parse_github(&github_info);
                        }
                        Err(err) => {
                            eprintln!(
                                "JSON error for {} [{}]: {}",
                                self.distname, self.master_site, err
                            );
                        }
                    }
                }
                _ => match client.get(&self.master_site).send().await {
                    Ok(response) => {
                        if response.status() != StatusCode::OK {
                            eprintln!("Status {}", response.status());
                            return false;
                        }
                        let body = response.text().await.unwrap();
                        match parse_html(&body) {
                            None => eprintln!("No version for {}", self.distname),
                            Some(version) => {
                                let my_version = Version::from_str(&self.version).unwrap();
                                if my_version < version {
                                    self.version = version.to_string();
                                }
                            }
                        }
                    }
                    Err(err) => eprintln!("Error fetching {}: {}", self.distname, err),
                },
            }
        }
        false
    }

    #[must_use]
    pub fn is_latest(&self) -> bool {
        if let Some(local) = &self.local_version {
            let local: Vec<i32> = local
                .split('.')
                .map_while(|d| i32::from_str(d).ok())
                .collect();
            let version = self
                .version
                .split('.')
                .map_while(|d| i32::from_str(d).ok())
                .collect();
            local >= version
        } else {
            false
        }
    }

    pub fn display_info(&self) {
        println!("Distname:      {}", self.distname);
        println!("Master site:   {}", self.master_site);
        println!("Version:       {}", self.version);
        println!(
            "Local version: {}",
            self.local_version.as_ref().unwrap_or(&"-".into())
        );
        println!("Last check:    {}", self.last_check);
    }
}

impl fmt::Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} -> {}",
            &self.distname,
            self.local_version.as_ref().unwrap_or(&"-".into()),
            &self.version
        )
    }
}
