use std::{fs::read, path::PathBuf};

use clap::{arg, command, value_parser, Command};
use sqlx::sqlite::SqlitePool;
use toml::{Table, Value};
use vert::package::Package;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = command!()
        .arg(
            arg!(-c --config <FILE> "configuration file")
                .required(false)
                .value_parser(value_parser!(PathBuf))
                .default_value("vert.toml"),
        )
        .arg(
            arg!(-d --db <FILE> "SQLite database file")
                .required(false)
                .default_value("vert.db"),
        )
        .arg_required_else_help(true)
        .propagate_version(true)
        .subcommand_required(true)
        .subcommand(
            Command::new("add")
                .about("Add package")
                .arg(arg!(-l --url <URL> "package master site").required(true))
                .arg(arg!(-r --release <VERSION> "locally installed version").required(true))
                .arg(arg!(<pkg> "package name")),
        )
        .subcommand(
            Command::new("check")
                .about("Check for new version")
                .arg(arg!([pkg] "package name")),
        )
        .subcommand(
            Command::new("delete")
                .about("Delete package")
                .arg(arg!(<pkg> "package name")),
        )
        .subcommand(
            Command::new("info")
                .about("Display information about package")
                .arg(arg!([pkg] "package name")),
        )
        .subcommand(
            Command::new("mark")
                .about("Mark as updated")
                .arg(arg!(<pkg> "package name")),
        )
        .subcommand(
            Command::new("update")
                .about("Update package")
                .arg(arg!(-l --url [URL] "package master site"))
                .arg(arg!(-n --name [NAME] "new package name"))
                .arg(arg!(-r --release [VERSION] "locally installed version"))
                .arg(arg!(<pkg> "package name")),
        )
        .get_matches();

    // TODO: database path from config
    let db_path = matches.get_one::<String>("db").expect("database path");
    let pool = SqlitePool::connect(&format!("sqlite:{db_path}")).await?;

    // read config
    let mut github_account = None;
    let mut github_token = None;
    if let Some(path) = matches.get_one::<PathBuf>("config") {
        if let Ok(data) = read(path) {
            let config: Table = String::from_utf8_lossy(&data).parse()?;
            if let Some(github) = config.get("github") {
                github_account = github.get("account").and_then(|value| {
                    if let Value::String(s) = value {
                        Some(s.clone())
                    } else {
                        None
                    }
                });
                github_token = github.get("token").and_then(|value| {
                    if let Value::String(s) = value {
                        Some(s.clone())
                    } else {
                        None
                    }
                });
            }
        }
    }

    match matches.subcommand() {
        Some(("add", submatches)) => {
            let pkg = Package::add(
                &pool,
                submatches
                    .get_one::<String>("pkg")
                    .expect("pkg is required")
                    .into(),
                submatches
                    .get_one::<String>("url")
                    .expect("url is required")
                    .into(),
                submatches
                    .get_one::<String>("release")
                    .expect("release is required")
                    .into(),
            )
            .await?;
            println!("added {pkg}");
            return Ok(());
        }
        Some(("check", submatches)) => {
            if let Some(name) = submatches.get_one::<String>("pkg") {
                let mut pkg = Package::fetch_by_name(&pool, name).await?;
                if pkg
                    .auto_check(github_account.as_ref(), github_token.as_ref())
                    .await
                {
                    pkg.store_version(&pool).await.unwrap();
                } else {
                    pkg.update_last_check(&pool).await.unwrap();
                }
                pkg.display_info();
            } else {
                Package::check_all(&pool, github_account.as_ref(), github_token.as_ref()).await;
            }
        }
        Some(("delete", submatches)) => {
            let name = submatches
                .get_one::<String>("pkg")
                .expect("pkg is required");
            let pkg = Package::fetch_by_name(&pool, name).await?;
            pkg.delete(&pool).await?;
        }
        Some(("info", submatches)) => {
            if let Some(name) = submatches.get_one::<String>("pkg") {
                let pkg = Package::fetch_by_name(&pool, name).await?;
                pkg.display_info();
            } else {
                Package::info_stream(&pool).await;
                let total = Package::total(&pool).await?;
                println!("Total {total}");
            }
        }
        Some(("mark", submatches)) => {
            let name = submatches
                .get_one::<String>("pkg")
                .expect("pkg is required");
            let mut pkg = Package::fetch_by_name(&pool, name).await?;
            pkg.mark_latest(&pool).await?;
        }
        Some(("update", submatches)) => {
            let name = submatches
                .get_one::<String>("pkg")
                .expect("pkg is required");
            let mut pkg = Package::fetch_by_name(&pool, name).await?;
            pkg.update(
                &pool,
                submatches.get_one::<String>("name").cloned(),
                submatches.get_one::<String>("url").cloned(),
                submatches.get_one::<String>("release").cloned(),
            )
            .await?;
        }
        _ => unreachable!(),
    }

    Ok(())
}
