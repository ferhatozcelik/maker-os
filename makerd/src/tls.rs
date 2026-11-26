//! HTTPS: cihazın kendi ürettiği (kendinden imzalı) sertifika.
//!
//! Tarayıcılar mikrofonu (getUserMedia) ve AudioWorklet'i yalnız güvenli sayfada (HTTPS ya da localhost)
//! açar; MakerOS yerel ağda `http://<ip>` ile açıldığı için ses girişi için HTTPS gerekir. Sertifika ilk
//! açılışta `<data-dir>/tls/` altına yazılır; ana makine adı, `<ad>.local`, localhost ve cihazın o anki
//! IP adresleri için geçerlidir. Yeni bir ad/IP çıkarsa (DHCP) sertifika yenilenir. Tarayıcı bir kez
//! "bağlantı gizli değil" uyarısı gösterir; kullanıcı devam edince sayfa güvenli sayılır.

use std::collections::BTreeSet;
use std::io;
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use tokio_rustls::rustls;

/// Sertifikanın kapsaması gereken adlar: ana makine adı, `.local`, localhost ve yerel IP'ler.
pub fn local_names() -> BTreeSet<String> {
    let mut s = BTreeSet::from(["localhost".to_string(), "127.0.0.1".to_string()]);
    if let Some(h) = nix::unistd::gethostname().ok().and_then(|h| h.into_string().ok()).filter(|h| !h.is_empty()) {
        s.insert(format!("{h}.local"));
        s.insert(h);
    }
    if let Ok(ifs) = nix::ifaddrs::getifaddrs() {
        for i in ifs {
            let ip: Option<IpAddr> = i.address.as_ref().and_then(|a| {
                a.as_sockaddr_in().map(|v| IpAddr::V4(v.ip())).or_else(|| a.as_sockaddr_in6().map(|v| IpAddr::V6(v.ip())))
            });
            match ip {
                Some(IpAddr::V6(v6)) if (v6.segments()[0] & 0xffc0) == 0xfe80 => {} // bağ-yerel: tarayıcı adreste kullanmaz
                Some(ip) => {
                    s.insert(ip.to_string());
                }
                None => {}
            }
        }
    }
    s
}

/// Sertifikayı yükler; yoksa ya da `names`'in hepsini kapsamıyorsa yenisini üretir.
pub fn load_or_create(dir: &Path, names: &BTreeSet<String>) -> io::Result<Arc<rustls::ServerConfig>> {
    let (cert, key, list) = (dir.join("cert.pem"), dir.join("key.pem"), dir.join("names"));
    let have: BTreeSet<String> = std::fs::read_to_string(&list).unwrap_or_default().lines().map(str::to_string).collect();
    let fresh = !cert.is_file() || !key.is_file() || !names.is_subset(&have);
    if fresh {
        // Eski adlar korunur (IP değişip geri gelince yine geçerli kalsın), yenileri eklenir.
        let all: BTreeSet<String> = have.union(names).cloned().collect();
        create(dir, &all)?;
        tracing::info!(names = ?all, "HTTPS sertifikası üretildi");
    }
    match config(&cert, &key) {
        Ok(c) => Ok(c),
        Err(e) if !fresh => {
            tracing::warn!(err = %e, "HTTPS sertifikası okunamadı; yenisi üretiliyor");
            create(dir, names)?;
            config(&cert, &key)
        }
        Err(e) => Err(e),
    }
}

fn create(dir: &Path, names: &BTreeSet<String>) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let err = |e: rcgen::Error| io::Error::other(e.to_string());
    std::fs::create_dir_all(dir)?;
    let mut p = rcgen::CertificateParams::new(names.iter().cloned().collect::<Vec<_>>()).map_err(err)?;
    p.distinguished_name.push(rcgen::DnType::CommonName, names.iter().find(|n| n.ends_with(".local")).cloned().unwrap_or_else(|| "MakerOS".into()));
    p.distinguished_name.push(rcgen::DnType::OrganizationName, "MakerOS");
    p.not_before = rcgen::date_time_ymd(2024, 1, 1);
    p.not_after = rcgen::date_time_ymd(2049, 12, 31);
    let k = rcgen::KeyPair::generate().map_err(err)?;
    let c = p.self_signed(&k).map_err(err)?;
    let write = |name: &str, data: &str, mode: u32| -> io::Result<()> {
        let tmp = dir.join(format!("{name}.tmp"));
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(mode).open(&tmp)?;
        io::Write::write_all(&mut f, data.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, dir.join(name))
    };
    write("key.pem", &k.serialize_pem(), 0o600)?;
    write("cert.pem", &c.pem(), 0o644)?;
    write("names", &(names.iter().cloned().collect::<Vec<_>>().join("\n") + "\n"), 0o644)
}

fn config(cert: &Path, key: &Path) -> io::Result<Arc<rustls::ServerConfig>> {
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    let err = |e: &dyn std::fmt::Display| io::Error::other(e.to_string());
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert).map_err(|e| err(&e))?.collect::<Result<_, _>>().map_err(|e| err(&e))?;
    let key = PrivateKeyDer::from_pem_file(key).map_err(|e| err(&e))?;
    let mut c = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| err(&e))?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| err(&e))?;
    c.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_reload_extend() {
        let dir = std::env::temp_dir().join(format!("mk-tls-{}", crate::util::random_hex(4)));
        let a: BTreeSet<String> = ["localhost", "127.0.0.1", "maker-os.local"].map(String::from).into();
        load_or_create(&dir, &a).unwrap();
        let c1 = std::fs::read(dir.join("cert.pem")).unwrap();
        load_or_create(&dir, &a).unwrap();
        assert_eq!(c1, std::fs::read(dir.join("cert.pem")).unwrap(), "aynı adlarla yenilenmemeli");
        let mut b = a.clone();
        b.insert("192.168.1.50".into());
        load_or_create(&dir, &b).unwrap();
        assert_ne!(c1, std::fs::read(dir.join("cert.pem")).unwrap(), "yeni IP ile yenilenmeli");
        let names = std::fs::read_to_string(dir.join("names")).unwrap();
        assert!(names.contains("192.168.1.50") && names.contains("maker-os.local"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
