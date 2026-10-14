//! TLS settings for PSN requests.
//!
//! Sony's update hosts present certificates that no system trusts, so certificate checks are
//! skipped for Sony's hosts only. Every other host, such as one a redirect points to, is checked
//! against the same root certificates reqwest uses by default. The handshake signature is still
//! checked on every host.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, RootCertStore, SignatureScheme};

/// Every host rusty-psn talks to is under this domain.
const SONY_DOMAIN: &str = ".playstation.net";

#[derive(Debug)]
struct SonyOnlyCertBypass {
    inner: Arc<WebPkiServerVerifier>,
}

impl ServerCertVerifier for SonyOnlyCertBypass {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if let ServerName::DnsName(name) = server_name {
            if name.as_ref().ends_with(SONY_DOMAIN) {
                return Ok(ServerCertVerified::assertion());
            }
        }

        self.inner
            .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// The TLS configuration for the PSN client.
pub fn client_config() -> rustls::ClientConfig {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let roots = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let inner = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider.clone())
        .build()
        // Fails only on an empty root store, and webpki-roots is never empty.
        .expect("webpki-roots provides root certificates");

    rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        // ring supports every default protocol version.
        .expect("ring supports the default TLS versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SonyOnlyCertBypass { inner }))
        .with_no_client_auth()
}
