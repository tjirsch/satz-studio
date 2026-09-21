//! The latest-release reads against a canned HTTP server on a local port — no test touches
//! the network: the look for a newer satz-studio and its three outcomes, and satz's
//! installer, fetched with its sidecar from one release, verified, and run only on a match.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use satz_studio_core::github::{self, GithubError, StudioUpdate};
use satz_studio_core::satz::install::{self, InstallError};

const TIME_BOX: Duration = Duration::from_secs(60);

/// One response the server gives to one connection.
struct Canned {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
}

impl Canned {
    fn json(status: u16, body: String) -> Self {
        Self {
            status,
            headers: vec![("content-type", "application/json".to_string())],
            body: body.into_bytes(),
        }
    }
}

/// A server that answers the responses `script` builds from its own base URL, one
/// connection each and in order, and keeps the request line of each.
struct Server {
    base_url: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl Server {
    fn start(script: impl FnOnce(&str) -> Vec<Canned>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
        let base_url = format!("http://{}", listener.local_addr().expect("an address"));
        let responses = script(&base_url);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        std::thread::spawn(move || {
            for canned in responses {
                let (mut stream, _) = listener.accept().expect("a connection");
                let head = read_head(&mut stream);
                seen.lock()
                    .unwrap()
                    .push(head.lines().next().unwrap_or_default().to_string());
                let mut response = format!(
                    "HTTP/1.1 {} Status\r\nconnection: close\r\ncontent-length: {}\r\n",
                    canned.status,
                    canned.body.len()
                );
                for (name, value) in &canned.headers {
                    response.push_str(&format!("{name}: {value}\r\n"));
                }
                response.push_str("\r\n");
                stream.write_all(response.as_bytes()).expect("writes");
                stream.write_all(&canned.body).expect("writes");
                stream.flush().expect("flushes");
            }
        });
        Self { base_url, requests }
    }

    fn request_lines(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

/// The request head; these are all GETs, so there is no body to read.
fn read_head(stream: &mut std::net::TcpStream) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = stream.read(&mut chunk).expect("reads");
        assert!(n > 0, "the client closed before the head ended");
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            return String::from_utf8_lossy(&buf).to_string();
        }
    }
}

fn release_json(tag: &str, html_url: &str, assets: &[(&str, String)]) -> String {
    serde_json::json!({
        "tag_name": tag,
        "html_url": html_url,
        "assets": assets
            .iter()
            .map(|(name, url)| serde_json::json!({ "name": name, "browser_download_url": url }))
            .collect::<Vec<_>>(),
    })
    .to_string()
}

async fn look(server: &Server, running: &str) -> Result<StudioUpdate, GithubError> {
    let running = semver::Version::parse(running).unwrap();
    tokio::time::timeout(
        TIME_BOX,
        github::look_for_studio_update(&github::client(), &server.base_url, &running),
    )
    .await
    .expect("the look ends")
}

#[tokio::test]
async fn a_newer_release_is_named_with_its_page_and_the_latest_is_read_never_a_tag() {
    let page = "https://github.com/tjirsch/satz-studio/releases/tag/v0.4.0";
    let server = Server::start(|_| vec![Canned::json(200, release_json("v0.4.0", page, &[]))]);
    let found = look(&server, "0.3.0").await.unwrap();
    assert_eq!(
        found,
        StudioUpdate::Available {
            version: semver::Version::new(0, 4, 0),
            page: page.to_string(),
        }
    );
    assert_eq!(
        server.request_lines(),
        ["GET /repos/tjirsch/satz-studio/releases/latest HTTP/1.1"]
    );
}

#[tokio::test]
async fn the_running_release_or_a_newer_build_is_the_latest() {
    let page = "https://github.com/tjirsch/satz-studio/releases/tag/v0.3.0";
    let server = Server::start(|_| {
        vec![
            Canned::json(200, release_json("v0.3.0", page, &[])),
            Canned::json(200, release_json("v0.3.0", page, &[])),
        ]
    });
    let latest = semver::Version::new(0, 3, 0);
    assert_eq!(
        look(&server, "0.3.0").await.unwrap(),
        StudioUpdate::Latest {
            latest: latest.clone()
        }
    );
    assert_eq!(
        look(&server, "0.3.1").await.unwrap(),
        StudioUpdate::Latest { latest }
    );
}

/// GitHub's unauthenticated limit is a 403 with the quota's headers; the look says it is
/// the rate limit and when it resets, rather than "error 403".
#[tokio::test]
async fn a_403_is_the_rate_limit_and_says_so() {
    let reset = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let server = Server::start(|_| {
        vec![Canned {
            status: 403,
            headers: vec![
                ("content-type", "application/json".to_string()),
                ("x-ratelimit-remaining", "0".to_string()),
                ("x-ratelimit-reset", reset.to_string()),
            ],
            body: br#"{"message":"API rate limit exceeded"}"#.to_vec(),
        }]
    });
    let e = look(&server, "0.3.0").await.unwrap_err();
    let said = e.to_string();
    assert!(matches!(e, GithubError::RateLimited { .. }), "{e:?}");
    assert!(said.contains("60 requests an hour"), "{said}");
    assert!(said.contains("resets in"), "{said}");
}

#[tokio::test]
async fn nothing_listening_is_unreachable_and_says_offline() {
    // a port that was bound and let go: nothing listens there any more
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let running = semver::Version::new(0, 3, 0);
    let e = tokio::time::timeout(
        TIME_BOX,
        github::look_for_studio_update(&github::client(), &base, &running),
    )
    .await
    .unwrap()
    .unwrap_err();
    let said = e.to_string();
    assert!(matches!(e, GithubError::Unreachable { .. }), "{e:?}");
    assert!(said.contains("offline"), "{said}");
}

#[tokio::test]
async fn no_release_and_a_tag_that_is_not_a_version_are_said_as_such() {
    let server = Server::start(|_| {
        vec![
            Canned::json(404, r#"{"message":"Not Found"}"#.to_string()),
            Canned::json(
                200,
                release_json("nightly", "https://github.com/tjirsch/satz-studio", &[]),
            ),
        ]
    });
    let e = look(&server, "0.3.0").await.unwrap_err();
    assert!(
        matches!(e, GithubError::NoRelease { ref repo } if repo == "tjirsch/satz-studio"),
        "{e:?}"
    );
    let e = look(&server, "0.3.0").await.unwrap_err();
    assert!(
        matches!(e, GithubError::Tag(ref t) if t == "nightly"),
        "{e:?}"
    );
}

// ---- satz's installer ------------------------------------------------------------

/// The fetch and the check run for both installers on every system; the run is the
/// system's own — `sh` off Windows, `powershell` on it.
mod installer {
    use satz_studio_core::satz::install::Installer;
    use sha2::{Digest, Sha256};

    use super::*;

    const BOTH: [Installer; 2] = [Installer::Shell, Installer::PowerShell];

    fn bytes(body: &[u8]) -> Canned {
        Canned {
            status: 200,
            headers: vec![("content-type", "application/octet-stream".to_string())],
            body: body.to_vec(),
        }
    }

    fn sidecar_of(installer: Installer, script: &[u8]) -> String {
        format!(
            "{}  {}\n",
            hex::encode(Sha256::digest(script)),
            installer.asset()
        )
    }

    /// A release of satz that publishes both installers with their sidecars, as satz does;
    /// the asset URLs point back at the same server, which answers the first download with
    /// `script` and the second with `sidecar`.
    fn satz_release(script: Vec<u8>, sidecar: String) -> Server {
        Server::start(move |base| {
            let assets: Vec<(&str, String)> = BOTH
                .iter()
                .flat_map(|i| [i.asset(), i.sidecar()])
                .map(|name| (name, format!("{base}/download/{name}")))
                .collect();
            vec![
                Canned::json(
                    200,
                    release_json(
                        "v0.59.7",
                        "https://github.com/tjirsch/satz/releases/tag/v0.59.7",
                        &assets,
                    ),
                ),
                bytes(&script),
                bytes(sidecar.as_bytes()),
            ]
        })
    }

    async fn fetch(
        installer: Installer,
        server: &Server,
    ) -> Result<install::VerifiedInstaller, InstallError> {
        tokio::time::timeout(
            TIME_BOX,
            install::fetch_verified(installer, &github::client(), &server.base_url),
        )
        .await
        .expect("the fetch ends")
    }

    #[tokio::test]
    async fn each_installer_and_its_sidecar_come_from_the_one_latest_release() {
        for installer in BOTH {
            let script = b"echo installed\n".to_vec();
            let server = satz_release(script.clone(), sidecar_of(installer, &script));
            let verified = fetch(installer, &server).await.unwrap();
            assert_eq!(verified.release, "v0.59.7");
            assert_eq!(verified.installer, installer);
            assert_eq!(verified.sha256, hex::encode(Sha256::digest(&script)));
            assert_eq!(
                server.request_lines(),
                [
                    "GET /repos/tjirsch/satz/releases/latest HTTP/1.1".to_string(),
                    format!("GET /download/{} HTTP/1.1", installer.asset()),
                    format!("GET /download/{} HTTP/1.1", installer.sidecar()),
                ]
            );
        }
    }

    #[tokio::test]
    async fn an_installer_that_does_not_match_its_sidecar_is_refused_and_never_runs() {
        for installer in BOTH {
            let server = satz_release(
                b"echo this release\n".to_vec(),
                // the sidecar of another script
                sidecar_of(installer, b"echo another release\n"),
            );
            let e = fetch(installer, &server).await.unwrap_err();
            let said = e.to_string();
            assert!(matches!(e, InstallError::Mismatch { .. }), "{e:?}");
            assert!(said.contains("nothing was run"), "{said}");
            assert!(said.contains(installer.asset()), "{said}");
        }
    }

    #[tokio::test]
    async fn a_release_without_a_sidecar_is_refused_before_anything_is_downloaded() {
        for installer in BOTH {
            let server = Server::start(|base| {
                vec![Canned::json(
                    200,
                    release_json(
                        "v0.59.7",
                        "https://github.com/tjirsch/satz/releases/tag/v0.59.7",
                        &[(
                            installer.asset(),
                            format!("{base}/download/{}", installer.asset()),
                        )],
                    ),
                )]
            });
            let e = fetch(installer, &server).await.unwrap_err();
            assert!(
                matches!(e, InstallError::NoSidecar { ref release, sidecar } if release == "v0.59.7" && sidecar == installer.sidecar()),
                "{e:?}"
            );
            assert_eq!(server.request_lines().len(), 1, "only the release was read");
        }
    }

    #[tokio::test]
    async fn a_sidecar_that_is_not_a_sha256_is_refused() {
        for installer in BOTH {
            let server = satz_release(
                b"echo installed\n".to_vec(),
                "<html>not found</html>".to_string(),
            );
            let e = fetch(installer, &server).await.unwrap_err();
            assert!(matches!(e, InstallError::SidecarUnreadable { .. }), "{e:?}");
        }
    }

    /// A script, in this system's installer language, that says what it is run with —
    /// the folder it is told, whether the `PATH` is left alone, whether stdin is closed —
    /// and leaves a mark where it ran.
    fn fake_installer(mark: &std::path::Path) -> Vec<u8> {
        let script = if cfg!(windows) {
            format!(
                "Write-Output \"install-dir=$env:SATZ_INSTALL_DIR\"\r\nWrite-Output \"no-modify-path=$env:SATZ_NO_MODIFY_PATH\"\r\nif ($null -eq [Console]::In.ReadLine()) {{ Write-Output 'stdin=closed' }} else {{ Write-Output 'stdin=open' }}\r\nNew-Item -ItemType File -Path '{}' | Out-Null\r\n",
                mark.display()
            )
        } else {
            format!(
                "#!/bin/sh\necho \"install-dir=$SATZ_INSTALL_DIR\"\necho \"no-modify-path=$SATZ_NO_MODIFY_PATH\"\nif read line; then echo \"stdin=open\"; else echo \"stdin=closed\"; fi\ntouch '{}'\n",
                mark.display()
            )
        };
        script.into_bytes()
    }

    #[tokio::test]
    async fn this_system_s_installer_runs_into_the_named_folder_without_a_path_edit_or_a_prompt() {
        use satz_studio_core::satz::CliLine;
        use tokio_util::sync::CancellationToken;

        let installer = Installer::for_this_system();
        let tmp = tempfile::tempdir().unwrap();
        let mark = tmp.path().join("ran");
        let bin = tmp.path().join("bin");
        let script = fake_installer(&mark);
        let server = satz_release(script.clone(), sidecar_of(installer, &script));
        let verified = fetch(installer, &server).await.unwrap();

        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let status =
            tokio::time::timeout(TIME_BOX, verified.run(&bin, tx, CancellationToken::new()))
                .await
                .expect("the installer ends")
                .unwrap();
        let mut out = Vec::new();
        let mut err = Vec::new();
        while let Some(line) = rx.recv().await {
            match line {
                CliLine::Stdout(s) => out.push(s),
                CliLine::Stderr(s) => err.push(s),
            }
        }
        assert!(status.success(), "{status}: {err:?}");
        assert_eq!(
            out,
            [
                format!("install-dir={}", bin.display()),
                "no-modify-path=1".to_string(),
                "stdin=closed".to_string(),
            ]
        );
        assert!(mark.is_file(), "the verified installer ran");
    }
}
