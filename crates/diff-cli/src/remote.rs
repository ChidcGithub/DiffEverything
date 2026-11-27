use anyhow::{Context, Result};
use base64::Engine;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use url::Url;

#[derive(Debug, Clone, Default)]
pub struct RemoteAuth {
    pub user: Option<String>,
    pub password: Option<String>,
    pub sftp_known_hosts: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct RemoteSpec {
    url: Url,
    webdav: bool,
}

impl RemoteSpec {
    fn parse(spec: &str) -> Option<Self> {
        let webdav = spec.starts_with("webdav://") || spec.starts_with("webdavs://");
        let normalized = if spec.starts_with("webdav://") {
            spec.replacen("webdav://", "http://", 1)
        } else if spec.starts_with("webdavs://") {
            spec.replacen("webdavs://", "https://", 1)
        } else {
            spec.to_string()
        };
        let url = Url::parse(&normalized).ok()?;
        if !matches!(url.scheme(), "http" | "https" | "sftp" | "smb") {
            return None;
        }
        url.host_str()?;
        Some(Self { url, webdav })
    }

    fn file_name(&self) -> String {
        self.url
            .path_segments()
            .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
            .map(|segment| segment.replace('%', "_").replace(['/', '\\', ':'], "_"))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "remote.bin".to_string())
    }

    fn credentials(&self, auth: &RemoteAuth) -> (Option<String>, Option<String>) {
        let user = auth
            .user
            .clone()
            .or_else(|| (!self.url.username().is_empty()).then(|| self.url.username().to_string()));
        let password = auth
            .password
            .clone()
            .or_else(|| self.url.password().map(str::to_string));
        (user, password)
    }

    fn fetch(&self, destination: &Path, auth: &RemoteAuth) -> Result<()> {
        match self.url.scheme() {
            "http" | "https" => self.fetch_http(destination, auth),
            "sftp" => self.fetch_sftp(destination, auth),
            "smb" => self.fetch_smb(destination, auth),
            _ => anyhow::bail!("unsupported remote scheme {}", self.url.scheme()),
        }
    }

    fn fetch_http(&self, destination: &Path, auth: &RemoteAuth) -> Result<()> {
        if self.webdav {
            return download_webdav(&self.url, destination, auth, 0);
        }
        let mut url = self.url.clone();
        let (user, password) = self.credentials(auth);
        if !url.username().is_empty() {
            let _ = url.set_username("");
            let _ = url.set_password(None);
        }

        let agent = ureq::AgentBuilder::new().build();
        let mut request = agent.get(url.as_str());
        if let Some(user) = user {
            let credentials = format!("{}:{}", user, password.unwrap_or_default());
            let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
            request = request.set("Authorization", &format!("Basic {encoded}"));
        }
        let response = request
            .call()
            .with_context(|| format!("WebDAV/HTTP GET failed for {}", self.url))?;
        let mut reader = response.into_reader();
        let mut file = fs::File::create(destination)?;
        std::io::copy(&mut reader, &mut file)?;
        Ok(())
    }

    fn fetch_smb(&self, destination: &Path, auth: &RemoteAuth) -> Result<()> {
        let host = self
            .url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("SMB URL has no host"))?;
        let port = self.url.port().unwrap_or(445);
        let segments: Vec<&str> = self
            .url
            .path_segments()
            .map(|segments| segments.filter(|segment| !segment.is_empty()).collect())
            .unwrap_or_default();
        let (share, remote_path) = segments
            .split_first()
            .ok_or_else(|| anyhow::anyhow!("SMB URL must include a share and a path"))?;
        if remote_path.is_empty() {
            anyhow::bail!("SMB URL must include a file or directory path after the share");
        }
        let remote_path = remote_path.join("/");
        let (user, password) = self.credentials(auth);
        let user = user.unwrap_or_default();
        let password = password.unwrap_or_default();

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            let address = format!("{host}:{port}");
            let mut client = smb2::connect(&address, &user, &password)
                .await
                .map_err(|error| anyhow::anyhow!("SMB connection failed: {error}"))?;
            let mut tree = client
                .connect_share(share)
                .await
                .map_err(|error| anyhow::anyhow!("SMB share connect failed: {error}"))?;
            let info = client
                .stat(&mut tree, &remote_path)
                .await
                .map_err(|error| anyhow::anyhow!("SMB stat failed: {error}"))?;
            if info.is_directory {
                fs::create_dir_all(destination)?;
                download_smb_dir(&mut client, &mut tree, &remote_path, destination, 0).await?;
            } else {
                let data = client
                    .read_file(&mut tree, &remote_path)
                    .await
                    .map_err(|error| anyhow::anyhow!("SMB read failed: {error}"))?;
                let mut output = fs::File::create(destination)?;
                output.write_all(&data)?;
            }
            client
                .disconnect_share(&tree)
                .await
                .map_err(|error| anyhow::anyhow!("SMB disconnect failed: {error}"))?;
            Ok::<(), anyhow::Error>(())
        })?;
        Ok(())
    }

    fn fetch_sftp(&self, destination: &Path, auth: &RemoteAuth) -> Result<()> {
        let host = self
            .url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("SFTP URL has no host"))?;
        let port = self.url.port_or_known_default().unwrap_or(22);
        let (user, password) = self.credentials(auth);
        let user =
            user.ok_or_else(|| anyhow::anyhow!("SFTP requires a username (URL or --remote-user)"))?;
        let tcp = TcpStream::connect((host, port))
            .with_context(|| format!("cannot connect to SFTP {host}:{port}"))?;
        let mut session = ssh2::Session::new()?;
        session.set_tcp_stream(tcp);
        session.handshake()?;
        if let Some(path) = &auth.sftp_known_hosts {
            let mut known_hosts = session.known_hosts()?;
            known_hosts
                .read_file(path, ssh2::KnownHostFileKind::OpenSSH)
                .with_context(|| format!("cannot read SFTP known_hosts file {}", path.display()))?;
            let key = session
                .host_key()
                .map(|(key, _)| key)
                .ok_or_else(|| anyhow::anyhow!("SFTP server did not present a host key"))?;
            match known_hosts.check_port(host, port, key) {
                ssh2::CheckResult::Match => {}
                ssh2::CheckResult::NotFound => anyhow::bail!(
                    "SFTP host key for {host}:{port} is not present in {}; add it before connecting",
                    path.display()
                ),
                ssh2::CheckResult::Mismatch => {
                    anyhow::bail!("SFTP host key mismatch for {host}:{port}; refusing to connect")
                }
                ssh2::CheckResult::Failure => {
                    anyhow::bail!("SFTP known_hosts check failed for {host}:{port}")
                }
            }
        }
        if let Some(password) = password {
            session
                .userauth_password(&user, &password)
                .with_context(|| format!("SFTP password authentication failed for {user}"))?;
        } else {
            session
                .userauth_agent(&user)
                .with_context(|| format!("SFTP agent authentication failed for {user}"))?;
        }
        if !session.authenticated() {
            anyhow::bail!("SFTP authentication failed for {user}");
        }
        let sftp = session.sftp()?;
        let remote_path = Path::new(self.url.path());
        let stat = sftp
            .stat(remote_path)
            .with_context(|| format!("cannot stat SFTP path {}", self.url.path()))?;
        if stat.is_dir() {
            fs::create_dir_all(destination)?;
            download_sftp_dir(&sftp, remote_path, destination)?;
        } else {
            let mut remote = sftp
                .open(remote_path)
                .with_context(|| format!("cannot open SFTP path {}", self.url.path()))?;
            let mut data = Vec::new();
            remote.read_to_end(&mut data)?;
            let mut file = fs::File::create(destination)?;
            file.write_all(&data)?;
        }
        Ok(())
    }
}

async fn download_smb_dir(
    client: &mut smb2::SmbClient,
    tree: &mut smb2::Tree,
    remote: &str,
    destination: &Path,
    depth: usize,
) -> Result<()> {
    if depth > 32 {
        anyhow::bail!("SMB directory recursion limit reached");
    }
    let entries = client
        .list_directory(tree, remote)
        .await
        .map_err(|error| anyhow::anyhow!("SMB directory listing failed for {remote}: {error}"))?;
    for entry in entries {
        if !safe_remote_name(&entry.name) || entry.is_link() {
            continue;
        }
        let child_remote = format!("{}/{}", remote.trim_end_matches('/'), entry.name);
        let child_local = destination.join(&entry.name);
        if entry.is_directory {
            fs::create_dir_all(&child_local)?;
            Box::pin(download_smb_dir(
                client,
                tree,
                &child_remote,
                &child_local,
                depth + 1,
            ))
            .await?;
        } else {
            let data = client
                .read_file(tree, &child_remote)
                .await
                .map_err(|error| anyhow::anyhow!("SMB read failed for {child_remote}: {error}"))?;
            let mut output = fs::File::create(&child_local)?;
            output.write_all(&data)?;
        }
    }
    Ok(())
}

/// Resolve a local path or download a remote URL into `temp_dir`.
/// Returns the local path and the display name (the original URL or path).
pub fn resolve_input(spec: &Path, temp_dir: &Path, auth: &RemoteAuth) -> Result<(PathBuf, String)> {
    let text = spec.to_string_lossy().into_owned();
    if let Some(remote) = RemoteSpec::parse(&text) {
        // Give each input its own directory so two URLs with the same final
        // path segment (for example /left/README.md and /right/README.md)
        // cannot overwrite each other in the shared temp directory.
        let unique = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            text.hash(&mut hasher);
            hasher.finish()
        };
        let root = temp_dir.join(format!("input-{unique:016x}"));
        fs::create_dir_all(&root)?;
        let destination = root.join(remote.file_name());
        remote
            .fetch(&destination, auth)
            .with_context(|| format!("failed to fetch remote input {text}"))?;
        Ok((destination, text))
    } else {
        Ok((spec.to_path_buf(), text))
    }
}

fn auth_header(url: &Url, auth: &RemoteAuth) -> (Url, Option<String>) {
    let mut clean_url = url.clone();
    let user = auth
        .user
        .clone()
        .or_else(|| (!clean_url.username().is_empty()).then(|| clean_url.username().to_string()));
    let password = auth
        .password
        .clone()
        .or_else(|| clean_url.password().map(str::to_string));
    if !clean_url.username().is_empty() {
        let _ = clean_url.set_username("");
        let _ = clean_url.set_password(None);
    }
    let header = user.map(|user| {
        let credentials = format!("{}:{}", user, password.unwrap_or_default());
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        format!("Basic {encoded}")
    });
    (clean_url, header)
}

fn get_http_file(url: &Url, destination: &Path, auth: &RemoteAuth) -> Result<()> {
    let (url, header) = auth_header(url, auth);
    let agent = ureq::AgentBuilder::new().build();
    let mut request = agent.get(url.as_str());
    if let Some(header) = header {
        request = request.set("Authorization", &header);
    }
    let response = request
        .call()
        .with_context(|| format!("HTTP GET failed for {url}"))?;
    let mut reader = response.into_reader();
    let mut file = fs::File::create(destination)?;
    std::io::copy(&mut reader, &mut file)?;
    Ok(())
}

#[derive(Debug)]
struct WebDavEntry {
    url: Url,
    name: String,
    is_dir: bool,
}

fn download_webdav(url: &Url, destination: &Path, auth: &RemoteAuth, depth: usize) -> Result<()> {
    if depth > 32 {
        anyhow::bail!("WebDAV directory recursion limit reached");
    }
    let Some(entries) = propfind(url, auth)? else {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        return get_http_file(url, destination, auth);
    };
    let base_path = url.path().trim_end_matches('/');
    let is_dir = entries
        .iter()
        .find(|entry| entry.url.path().trim_end_matches('/') == base_path)
        .map(|entry| entry.is_dir)
        .unwrap_or_else(|| !entries.is_empty() || url.path().ends_with('/'));
    if !is_dir {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        return get_http_file(url, destination, auth);
    }

    fs::create_dir_all(destination)?;
    for entry in entries {
        if entry.url.path().trim_end_matches('/') == base_path {
            continue;
        }
        if !safe_remote_name(&entry.name) {
            continue;
        }
        let target = destination.join(&entry.name);
        if entry.is_dir {
            download_webdav(&entry.url, &target, auth, depth + 1)?;
        } else {
            get_http_file(&entry.url, &target, auth)?;
        }
    }
    Ok(())
}

fn propfind(url: &Url, auth: &RemoteAuth) -> Result<Option<Vec<WebDavEntry>>> {
    let (url, header) = auth_header(url, auth);
    let agent = ureq::AgentBuilder::new().build();
    let mut request = agent
        .request("PROPFIND", url.as_str())
        .set("Depth", "1")
        .set("Content-Type", "application/xml; charset=utf-8");
    if let Some(header) = header {
        request = request.set("Authorization", &header);
    }
    let body = r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/></d:prop></d:propfind>"#;
    let response = match request.send_string(body) {
        Ok(response) => response,
        Err(ureq::Error::Status(404 | 405 | 501, _)) => return Ok(None),
        Err(error) => return Err(anyhow::anyhow!("WebDAV PROPFIND failed for {url}: {error}")),
    };
    if response.status() != 207 {
        return Ok(None);
    }
    let body = response
        .into_string()
        .map_err(|error| anyhow::anyhow!("WebDAV response read failed: {error}"))?;
    parse_webdav_entries(&body, &url).map(Some)
}

fn parse_webdav_entries(body: &str, base: &Url) -> Result<Vec<WebDavEntry>> {
    let mut reader = quick_xml::Reader::from_str(body);
    reader.config_mut().trim_text(true);
    let mut entries = Vec::new();
    let mut in_response = false;
    let mut in_href = false;
    let mut href = String::new();
    let mut is_dir = false;

    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Start(event)) => match event.local_name().as_ref() {
                "response" => {
                    in_response = true;
                    href.clear();
                    is_dir = false;
                }
                "href" => in_href = true,
                "collection" => is_dir = true,
                _ => {}
            },
            Ok(quick_xml::events::Event::Empty(event)) => {
                if event.local_name().as_ref() == "collection" {
                    is_dir = true;
                }
            }
            Ok(quick_xml::events::Event::Text(event)) if in_href => {
                href.push_str(&event.xml_content(quick_xml::XmlVersion::Implicit1_0));
            }
            Ok(quick_xml::events::Event::End(event)) => match event.local_name().as_ref() {
                "href" => in_href = false,
                "response" if in_response => {
                    in_response = false;
                    let href = href.trim();
                    if href.is_empty() {
                        continue;
                    }
                    let Ok(entry_url) = base.join(href) else {
                        continue;
                    };
                    // Never follow a listing to another origin or outside the
                    // directory being listed; either could leak credentials or
                    // fetch unrelated files.
                    if entry_url.scheme() != base.scheme()
                        || entry_url.host_str() != base.host_str()
                        || entry_url.port_or_known_default() != base.port_or_known_default()
                    {
                        continue;
                    }
                    let base_path = base.path().trim_end_matches('/');
                    let entry_path = entry_url.path().trim_end_matches('/');
                    if entry_path != base_path && !entry_path.starts_with(&format!("{base_path}/"))
                    {
                        continue;
                    }
                    let name = entry_url
                        .path_segments()
                        .and_then(|mut segments| {
                            segments
                                .rfind(|segment| !segment.is_empty())
                                .map(str::to_string)
                        })
                        .unwrap_or_default();
                    entries.push(WebDavEntry {
                        url: entry_url,
                        name,
                        is_dir,
                    });
                }
                _ => {}
            },
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(anyhow::anyhow!("invalid WebDAV XML: {error}")),
        }
    }
    Ok(entries)
}

fn download_sftp_dir(sftp: &ssh2::Sftp, remote: &Path, destination: &Path) -> Result<()> {
    for (path, stat) in sftp
        .readdir(remote)
        .with_context(|| format!("cannot list SFTP directory {}", remote.display()))?
    {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if !safe_remote_name(name) || stat.file_type().is_symlink() {
            continue;
        }
        let target = destination.join(name);
        if stat.is_dir() {
            fs::create_dir_all(&target)?;
            download_sftp_dir(sftp, &path, &target)?;
        } else {
            let mut remote_file = sftp
                .open(&path)
                .with_context(|| format!("cannot open SFTP file {}", path.display()))?;
            let mut data = Vec::new();
            remote_file.read_to_end(&mut data)?;
            let mut file = fs::File::create(&target)?;
            file.write_all(&data)?;
        }
    }
    Ok(())
}

fn safe_remote_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\'])
}

pub fn is_remote(spec: &Path) -> bool {
    RemoteSpec::parse(&spec.to_string_lossy()).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_remote_schemes() {
        assert!(is_remote(Path::new("http://example.com/a.txt")));
        assert!(is_remote(Path::new("webdav://example.com/a.txt")));
        assert!(is_remote(Path::new("sftp://user@example.com/a.txt")));
        assert!(!is_remote(Path::new("C:\\local\\a.txt")));
    }

    #[test]
    fn rejects_unsafe_remote_file_names() {
        assert!(safe_remote_name("README.md"));
        assert!(safe_remote_name("nested-file.txt"));
        assert!(!safe_remote_name(""));
        assert!(!safe_remote_name("."));
        assert!(!safe_remote_name(".."));
        assert!(!safe_remote_name("../../escape"));
        assert!(!safe_remote_name("..\\escape"));
        assert!(!safe_remote_name("dir/file"));
    }

    #[test]
    fn file_name_uses_url_path() {
        let spec = RemoteSpec::parse("https://example.com/files/old%20file.txt").unwrap();
        assert_eq!(spec.file_name(), "old_20file.txt");
    }

    #[test]
    fn webdav_listing_rejects_foreign_origins_and_siblings() {
        let base = Url::parse("http://example.com/data/left/").unwrap();
        let body = r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:">
            <d:response><d:href>/data/left/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat></d:response>
            <d:response><d:href>/data/left/ok.txt</d:href><d:propstat><d:prop><d:resourcetype/></d:prop></d:propstat></d:response>
            <d:response><d:href>/data/right/secret.txt</d:href><d:propstat><d:prop><d:resourcetype/></d:prop></d:propstat></d:response>
            <d:response><d:href>http://evil.example/data/left/steal.txt</d:href><d:propstat><d:prop><d:resourcetype/></d:prop></d:propstat></d:response>
        </d:multistatus>"#;
        let entries = parse_webdav_entries(body, &base).unwrap();
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, vec!["left", "ok.txt"]);
    }
}
