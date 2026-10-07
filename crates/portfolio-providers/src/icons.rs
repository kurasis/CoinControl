//! Anonymous, optional logos from a fixed public catalog. Never accepts URLs or credentials.
use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use portfolio_core::address::{normalize_address, ton_to_friendly};
use portfolio_core::network::NetworkId;
use portfolio_store::ingest::AssetSpec;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use url::Url;

use crate::network_log::NetworkLog;

const MAX_BYTES: usize = 128 * 1024;
const MAX_ENTRIES: usize = 512;
// Per UTC day within this application process; restarting resets this anonymous budget.
const DAILY_REQUESTS: u32 = 128;
const NEGATIVE_TTL: Duration = Duration::from_secs(3600);

/// Identity, never symbol/name, chooses the catalog path. Missing entries use a monogram.
fn catalog_url(asset: &AssetSpec) -> Option<Url> {
    let chain = match asset.network {
        NetworkId::Bitcoin => "bitcoin",
        NetworkId::Ethereum => "ethereum",
        NetworkId::Base => "base",
        NetworkId::Arbitrum => "arbitrum",
        NetworkId::Optimism => "optimism",
        NetworkId::Polygon => "polygon",
        NetworkId::Bsc => "smartchain",
        NetworkId::Solana => "solana",
        NetworkId::Tron => "tron",
        NetworkId::Ton => "ton",
    };
    let path = if let Some(contract) = &asset.contract {
        if asset.network == NetworkId::Bitcoin {
            return None;
        }
        let address = normalize_address(asset.network, contract).ok()?;
        let display = if asset.network == NetworkId::Ton {
            ton_to_friendly(&address.canonical, true).ok()?
        } else {
            address.display
        };
        format!("{chain}/assets/{display}/logo.png")
    } else {
        let chain = match asset.network {
            NetworkId::Base | NetworkId::Arbitrum | NetworkId::Optimism => "ethereum",
            _ => chain,
        };
        format!("{chain}/info/logo.png")
    };
    Url::parse(&format!(
        "https://raw.githubusercontent.com/trustwallet/assets/master/blockchains/{path}"
    ))
    .ok()
}

#[derive(Default)]
struct CacheState {
    entries: HashMap<String, (Instant, Option<String>)>,
    day: u64,
    requests: u32,
}

pub struct IconCache {
    directory: PathBuf,
    client: tokio::sync::OnceCell<reqwest::Client>,
    log: Arc<NetworkLog>,
    // One optional request at a time; this also coalesces duplicate requests and clear.
    state: Mutex<CacheState>,
}

impl IconCache {
    pub fn new(directory: PathBuf, log: Arc<NetworkLog>) -> Self {
        Self {
            directory,
            // Native certificate loading stays off the first-screen critical path.
            client: tokio::sync::OnceCell::new(),
            log,
            state: Mutex::new(CacheState::default()),
        }
    }

    pub async fn get(&self, asset: &AssetSpec) -> Option<String> {
        self.get_url(catalog_url(asset)?).await
    }

    async fn get_url(&self, url: Url) -> Option<String> {
        let key = format!("{:x}", Sha256::digest(url.as_str().as_bytes()));
        let mut state = self.state.lock().await;
        if let Some((at, value)) = state.entries.get(&key)
            && (value.is_some() || at.elapsed() < NEGATIVE_TTL)
        {
            return value.clone();
        }
        let file = self.directory.join(format!("{key}.png"));
        let cached = if tokio::fs::metadata(&file)
            .await
            .is_ok_and(|metadata| metadata.len() <= MAX_BYTES as u64)
        {
            tokio::fs::read(&file).await.ok()
        } else {
            None
        };
        let mut png = if let Some(bytes) = cached {
            sanitize(bytes).await
        } else {
            None
        };
        if png.is_none() {
            // Discard corrupt old entries before an atomic Windows rename.
            let _ = tokio::fs::remove_file(&file).await;
            let day = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() / 86400;
            if state.day != day {
                state.day = day;
                state.requests = 0;
            }
            if state.requests < DAILY_REQUESTS {
                state.requests += 1;
                png = self.fetch(&url).await;
                if let Some(bytes) = &png {
                    // Disk quota is bounded too. Failure never affects holdings or accounting.
                    if tokio::fs::create_dir_all(&self.directory).await.is_ok()
                        && disk_entries(&self.directory).await < MAX_ENTRIES
                    {
                        let temporary = self.directory.join(format!("{key}.tmp"));
                        if tokio::fs::write(&temporary, bytes).await.is_ok() {
                            let _ = tokio::fs::rename(&temporary, &file).await;
                            let _ = tokio::fs::remove_file(&temporary).await;
                        }
                    }
                }
            }
        }
        let value = png.map(|bytes| {
            format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )
        });
        if state.entries.len() >= MAX_ENTRIES {
            // Bounded memory; disk still serves successful evicted entries without network.
            if let Some(oldest) = state
                .entries
                .iter()
                .min_by_key(|(_, (at, _))| *at)
                .map(|(key, _)| key.clone())
            {
                state.entries.remove(&oldest);
            }
        }
        state.entries.insert(key, (Instant::now(), value.clone()));
        value
    }

    async fn fetch(&self, url: &Url) -> Option<Vec<u8>> {
        let started = Instant::now();
        let id = self.log.begin("token-icons", "GET", url, "token_icon", 1);
        let mut status = None;
        let result = async {
            let client = self
                .client
                .get_or_try_init(|| async {
                    tokio::task::spawn_blocking(|| {
                        reqwest::Client::builder()
                            .redirect(reqwest::redirect::Policy::none())
                            .timeout(Duration::from_secs(5))
                            .no_gzip()
                            .build()
                    })
                    .await
                    .map_err(|_| ())?
                    .map_err(|_| ())
                })
                .await
                .ok()?;
            let mut response = client.get(url.clone()).send().await.ok()?;
            status = Some(response.status().as_u16());
            if !response.status().is_success()
                || response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)?
                    .to_str()
                    .ok()?
                    .split(';')
                    .next()?
                    != "image/png"
                || response
                    .content_length()
                    .is_some_and(|length| length > MAX_BYTES as u64)
            {
                return None;
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.ok()? {
                if bytes.len() + chunk.len() > MAX_BYTES {
                    return None;
                }
                bytes.extend_from_slice(&chunk);
            }
            sanitize(bytes).await
        }
        .await;
        if let Some(id) = id {
            self.log.finish(
                id,
                if result.is_some() {
                    "success"
                } else {
                    "unavailable"
                },
                status,
                None,
                started,
            );
        }
        result
    }

    pub async fn clear(&self) -> std::io::Result<()> {
        let mut state = self.state.lock().await;
        state.entries.clear();
        // Keep the request budget when clearing images.
        match tokio::fs::remove_dir_all(&self.directory).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

async fn disk_entries(directory: &std::path::Path) -> usize {
    let Ok(mut entries) = tokio::fs::read_dir(directory).await else {
        return MAX_ENTRIES;
    };
    let mut count = 0;
    while entries.next_entry().await.ok().flatten().is_some() {
        count += 1;
        if count >= MAX_ENTRIES {
            break;
        }
    }
    count
}

async fn sanitize(bytes: Vec<u8>) -> Option<Vec<u8>> {
    tokio::task::spawn_blocking(move || rasterize(&bytes))
        .await
        .ok()
        .flatten()
}

/// Decode under limits and re-encode only 64x64 RGBA pixels. Metadata/APNG never reaches UI.
fn rasterize(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() > MAX_BYTES {
        return None;
    }
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_limits(png::Limits {
        bytes: 2 * 1024 * 1024,
    });
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let info = reader.info();
    if info.width == 0
        || info.height == 0
        || info.width > 512
        || info.height > 512
        || info.animation_control.is_some()
    {
        return None;
    }
    let mut pixels = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut pixels).ok()?;
    reader.finish().ok()?;
    let channels = frame.color_type.samples();
    let mut rgba = vec![0; 64 * 64 * 4];
    for y in 0..64 {
        for x in 0..64 {
            let source = ((y * frame.height as usize / 64) * frame.width as usize
                + x * frame.width as usize / 64)
                * channels;
            let target = &mut rgba[(y * 64 + x) * 4..][..4];
            match frame.color_type {
                png::ColorType::Rgba => target.copy_from_slice(&pixels[source..source + 4]),
                png::ColorType::Rgb => {
                    target[..3].copy_from_slice(&pixels[source..source + 3]);
                    target[3] = 255;
                }
                png::ColorType::Grayscale => {
                    target[..3].fill(pixels[source]);
                    target[3] = 255;
                }
                png::ColorType::GrayscaleAlpha => {
                    target[..3].fill(pixels[source]);
                    target[3] = pixels[source + 1];
                }
                png::ColorType::Indexed => return None,
            }
        }
    }
    let mut output = Vec::new();
    let mut encoder = png::Encoder::new(&mut output, 64, 64);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().ok()?.write_image_data(&rgba).ok()?;
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

    fn sample(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .add_text_chunk("untrusted".into(), "<script>secret</script>".into())
            .unwrap();
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&vec![255; width as usize * height as usize * 4])
            .unwrap();
        bytes
    }

    #[test]
    fn raster_strips_metadata_and_trailing_content_and_enforces_limits() {
        let mut bytes = sample(2, 3);
        bytes.extend_from_slice(b"<script>trailing payload</script>");
        let clean = rasterize(&bytes).unwrap();
        assert!(!clean.windows(6).any(|part| part == b"script"));
        let reader = png::Decoder::new(Cursor::new(&clean)).read_info().unwrap();
        assert_eq!((reader.info().width, reader.info().height), (64, 64));
        assert_eq!(reader.info().color_type, png::ColorType::Rgba);
        assert!(reader.info().uncompressed_latin1_text.is_empty());
        assert!(rasterize(b"<svg onload='alert(1)'/>").is_none());
        assert!(rasterize(&sample(513, 1)).is_none());
        assert!(rasterize(&vec![0; MAX_BYTES + 1]).is_none());
        assert!(rasterize(&bytes[..30]).is_none());
    }

    #[test]
    fn catalog_identity_is_validated_and_fixed_host() {
        for network in NetworkId::ALL {
            let asset = AssetSpec::native(network, "test");
            let url = catalog_url(&asset).unwrap();
            assert_eq!(url.host_str(), Some("raw.githubusercontent.com"));
            assert_eq!(url.scheme(), "https");
            assert_eq!(url.username(), "");
            assert!(url.query().is_none());
            if matches!(
                network,
                NetworkId::Base | NetworkId::Arbitrum | NetworkId::Optimism
            ) {
                assert!(url.path().ends_with("ethereum/info/logo.png"));
            }
        }
        let mut token = AssetSpec::native(NetworkId::Ethereum, "test");
        token.contract = Some("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".into());
        assert!(
            catalog_url(&token)
                .unwrap()
                .path()
                .contains("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48")
        );
        for malicious in [
            "../../secret",
            "https://evil.example/a",
            "0x1234",
            "//127.0.0.1",
        ] {
            token.contract = Some(malicious.into());
            assert!(catalog_url(&token).is_none());
        }
    }

    #[tokio::test]
    async fn coalesces_caches_offline_and_clears_without_resetting_budget() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(sample(2, 2))
                    .insert_header("content-type", "image/png"),
            )
            .expect(1)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let log = Arc::new(NetworkLog::default());
        log.set_enabled(true);
        let cache = IconCache::new(dir.path().join("icons"), log.clone());
        let url = Url::parse(&server.uri()).unwrap();
        let (a, b) = tokio::join!(cache.get_url(url.clone()), cache.get_url(url.clone()));
        assert!(a.as_ref().unwrap().starts_with("data:image/png;base64,"));
        assert_eq!(a, b);
        assert_eq!(log.entries().len(), 1);
        let requests = server.received_requests().await.unwrap();
        assert!(!requests[0].headers.contains_key("authorization"));
        assert!(!requests[0].headers.contains_key("cookie"));
        assert!(requests[0].body.is_empty());
        server.verify().await;
        server.reset().await;
        let restarted = IconCache::new(dir.path().join("icons"), log.clone());
        assert_eq!(a, restarted.get_url(url.clone()).await);
        assert_eq!(log.entries().len(), 1);
        cache.clear().await.unwrap();
        assert!(!dir.path().join("icons").exists());
        assert_eq!(cache.state.lock().await.requests, 1);
    }

    #[tokio::test]
    async fn rejects_redirects_html_oversized_bodies_and_caches_failures() {
        for response in [
            ResponseTemplate::new(302).insert_header("location", "http://127.0.0.1:1/secret"),
            ResponseTemplate::new(200)
                .set_body_string("<html>unsafe</html>")
                .insert_header("content-type", "text/html"),
            ResponseTemplate::new(200)
                .set_body_bytes(vec![0; MAX_BYTES + 1])
                .insert_header("content-type", "image/png"),
            ResponseTemplate::new(200)
                .set_body_bytes(b"not png")
                .insert_header("content-type", "image/png"),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .respond_with(response)
                .expect(1)
                .mount(&server)
                .await;
            let dir = tempfile::tempdir().unwrap();
            let cache = IconCache::new(dir.path().join("icons"), Arc::new(NetworkLog::default()));
            let url = Url::parse(&server.uri()).unwrap();
            assert!(cache.get_url(url.clone()).await.is_none());
            assert!(cache.get_url(url).await.is_none());
            server.verify().await;
        }
    }

    #[tokio::test]
    async fn exhausted_optional_budget_never_requests_network() {
        let server = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let cache = IconCache::new(dir.path().join("icons"), Arc::new(NetworkLog::default()));
        let mut state = cache.state.lock().await;
        state.day = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            / 86400;
        state.requests = DAILY_REQUESTS;
        drop(state);
        assert!(
            cache
                .get_url(Url::parse(&server.uri()).unwrap())
                .await
                .is_none()
        );
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
