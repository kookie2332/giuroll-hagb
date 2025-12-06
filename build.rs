use std::env;

use std::path::Path;

use winresource::{VersionInfo, WindowsResource};

extern crate winresource;

static VERSION_REMARK: Option<&str> = Some("(fork by Hagb)");
static DLL_REVISION: u16 = 3;
fn main() {
    let mut version = 0_u64;
    version |= env::var("CARGO_PKG_VERSION_MAJOR")
        .unwrap()
        .parse::<u64>()
        .unwrap()
        << 48;
    version |= env::var("CARGO_PKG_VERSION_MINOR")
        .unwrap()
        .parse::<u64>()
        .unwrap()
        << 32;
    version |= env::var("CARGO_PKG_VERSION_PATCH")
        .unwrap()
        .parse::<u64>()
        .unwrap()
        << 16;
    version |= DLL_REVISION as u64;

    println!("cargo:rustc-env=DLL_REVISION={}", DLL_REVISION);
    if let Some(remark) = VERSION_REMARK {
        println!("cargo:rustc-env=VERSION_REMARK={}", remark);
    }
    println!("cargo:rustc-env=DLL_VERSION={}", version);

    if env::var("CARGO_CFG_WINDOWS").is_err() {
        println!("cargo:warning=Skipping winresource because target platform is not Windows");
        return;
    }

    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
        && env::var("HOST").map(|h| !h.contains("windows")).unwrap_or(false)
    {
        println!("cargo:warning=Skipping winresource because MSVC resource tools are unavailable on this host");
        return;
    }

    if !Path::new("resource.rc").exists() {
        println!("cargo:warning=Skipping winresource because resource.rc is missing");
        return;
    }

    let mut res = WindowsResource::new();
    if cfg!(unix) {
        let ar_path = "/usr/i686-w64-mingw32/bin/ar";
        let windres_path = "/usr/bin/i686-w64-mingw32-windres";

        if Path::new(ar_path).exists() {
            res.set_ar_path(ar_path);
        }

        if Path::new(windres_path).exists() {
            res.set_windres_path(windres_path);
        }
    }

    res.set_version_info(VersionInfo::FILEVERSION, version);
    res.set_version_info(VersionInfo::PRODUCTVERSION, version);

    res.set(
        "LegalCopyright",
        format!(
            "Copyright (c) {}",
            env::var("CARGO_PKG_AUTHORS")
                .unwrap()
                .split(":")
                .collect::<Vec<_>>()
                .join(", ")
        )
        .as_str(),
    );
    res.set("ProductName", env::var("CARGO_PKG_NAME").unwrap().as_str());
    res.set(
        "FileDescription",
        env::var("CARGO_PKG_DESCRIPTION").unwrap().as_str(),
    );
    res.set(
        "ProductVersion",
        format!(
            "{}{}{}",
            env::var("CARGO_PKG_VERSION").unwrap(),
            match VERSION_REMARK {
                Some(remark) => " ".to_string() + &remark,
                None => "".to_string(),
            },
            env::var("SOURCE_URL")
                .and_then(|x| Ok(format!(" ({})", x).to_string()))
                .unwrap_or("".to_string())
        )
        .as_str(),
    );

    if let Err(e) = res.compile() {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}
