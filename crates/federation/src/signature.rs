//! The HTTP `Signature` envelope.
//!
//! Signing and verification are ports: an instance chooses its key custody and crypto
//! provider. The crate owns the header grammar and the canonical signing string.

use std::fmt;

/// Why a signature header or signing string was invalid.
#[derive(Debug, thiserror::Error)]
pub enum SignatureError {
    /// A required parameter is missing.
    #[error("signature is missing {0}")]
    Missing(&'static str),
    /// The header is not `key="value"` pairs.
    #[error("malformed signature header")]
    Malformed,
    /// The algorithm is not supported by the caller.
    #[error("unsupported signature algorithm: {0}")]
    Algorithm(String),
    /// The signing string has no headers.
    #[error("signature has no headers")]
    Empty,
    /// Key material or a cryptographic operation failed.
    #[error("signature crypto failed: {0}")]
    Crypto(String),
}

/// A parsed `Signature` / `Authorization: Signature` header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpSignature {
    /// Key id, normally an actor's `#main-key`.
    pub key_id: String,
    /// `rsa-sha256`, `ed25519`, etc.
    pub algorithm: String,
    /// Header names in signing order.
    pub headers: Vec<String>,
    /// Base64 signature.
    pub signature: String,
}

impl HttpSignature {
    /// Parse the parameters of an HTTP signature header.
    ///
    /// # Errors
    /// [`SignatureError::Malformed`] when a parameter is not quoted `key="value"`.
    pub fn parse(header: &str) -> Result<Self, SignatureError> {
        let mut key_id = None;
        let mut algorithm = None;
        let mut headers = None;
        let mut signature = None;
        for part in header.split(',') {
            let (name, value) = part.split_once('=').ok_or(SignatureError::Malformed)?;
            let value = value.trim().trim_matches('"').to_owned();
            match name.trim() {
                "keyId" => key_id = Some(value),
                "algorithm" => algorithm = Some(value),
                "headers" => headers = Some(value.split_whitespace().map(str::to_owned).collect()),
                "signature" => signature = Some(value),
                _ => {}
            }
        }
        Ok(Self {
            key_id: key_id.ok_or(SignatureError::Missing("keyId"))?,
            algorithm: algorithm.unwrap_or_else(|| "rsa-sha256".to_owned()),
            headers: headers.ok_or(SignatureError::Missing("headers"))?,
            signature: signature.ok_or(SignatureError::Missing("signature"))?,
        })
    }

    /// The canonical string a signer signs.
    ///
    /// `request` is called with each lower-case header name in the declared order.
    ///
    /// # Errors
    /// [`SignatureError::Empty`] when no headers were declared.
    pub fn signing_string<F>(&self, mut request: F) -> Result<String, SignatureError>
    where
        F: FnMut(&str) -> Option<String>,
    {
        if self.headers.is_empty() {
            return Err(SignatureError::Empty);
        }
        let mut lines = Vec::with_capacity(self.headers.len());
        for header in &self.headers {
            let value = request(header).ok_or(SignatureError::Malformed)?;
            lines.push(format!("{header}: {value}"));
        }
        Ok(lines.join("\n"))
    }
}

impl fmt::Display for HttpSignature {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "keyId=\"{}\",algorithm=\"{}\",headers=\"{}\",signature=\"{}\"",
            self.key_id,
            self.algorithm,
            self.headers.join(" "),
            self.signature
        )
    }
}

/// A signer supplied by the host.
pub trait Signer: Send + Sync {
    /// Sign a canonical string.
    fn sign(&self, key_id: &str, algorithm: &str, message: &[u8])
    -> Result<String, SignatureError>;
}

/// A verifier supplied by the host.
pub trait Verifier: Send + Sync {
    /// Verify a canonical string.
    fn verify(
        &self,
        key_id: &str,
        algorithm: &str,
        message: &[u8],
        signature: &str,
    ) -> Result<(), SignatureError>;
}

/// RSA-SHA256 signing and verification.
///
/// Key custody stays with the host: this type only wraps a parsed key, it does
/// not fetch or persist one.
#[cfg(feature = "crypto")]
pub mod crypto {
    use base64::Engine as _;
    use rsa::pkcs1v15::{Signature, SigningKey, VerifyingKey};
    use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
    use rsa::signature::{SignatureEncoding, Signer as _, Verifier as _};
    use sha2::{Digest, Sha256};

    use super::{SignatureError, Signer, Verifier};

    /// An RSA private key that signs `rsa-sha256`.
    #[derive(Debug)]
    pub struct RsaSigner {
        key: SigningKey<Sha256>,
    }

    impl RsaSigner {
        /// Parse a PKCS#8 PEM private key.
        ///
        /// # Errors
        /// [`SignatureError::Crypto`] when the PEM or key is invalid.
        pub fn from_pkcs8_pem(pem: &str) -> Result<Self, SignatureError> {
            let key = rsa::RsaPrivateKey::from_pkcs8_pem(pem)
                .map_err(|error| SignatureError::Crypto(error.to_string()))?;
            Ok(Self {
                key: SigningKey::new(key),
            })
        }
    }

    impl Signer for RsaSigner {
        fn sign(
            &self,
            _key_id: &str,
            algorithm: &str,
            message: &[u8],
        ) -> Result<String, SignatureError> {
            if algorithm != "rsa-sha256" {
                return Err(SignatureError::Algorithm(algorithm.to_owned()));
            }
            let signature = self.key.sign(message);
            Ok(base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()))
        }
    }

    /// An RSA public key that verifies `rsa-sha256`.
    #[derive(Debug)]
    pub struct RsaVerifier {
        key: VerifyingKey<Sha256>,
    }

    impl RsaVerifier {
        /// Parse a PKCS#8 PEM public key.
        ///
        /// # Errors
        /// [`SignatureError::Crypto`] when the PEM or key is invalid.
        pub fn from_public_key_pem(pem: &str) -> Result<Self, SignatureError> {
            let key = rsa::RsaPublicKey::from_public_key_pem(pem)
                .map_err(|error| SignatureError::Crypto(error.to_string()))?;
            Ok(Self {
                key: VerifyingKey::new(key),
            })
        }
    }

    impl Verifier for RsaVerifier {
        fn verify(
            &self,
            _key_id: &str,
            algorithm: &str,
            message: &[u8],
            signature: &str,
        ) -> Result<(), SignatureError> {
            if algorithm != "rsa-sha256" {
                return Err(SignatureError::Algorithm(algorithm.to_owned()));
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(signature)
                .map_err(|error| SignatureError::Crypto(error.to_string()))?;
            let signature = Signature::try_from(bytes.as_slice())
                .map_err(|error| SignatureError::Crypto(error.to_string()))?;
            self.key
                .verify(message, &signature)
                .map_err(|error| SignatureError::Crypto(error.to_string()))
        }
    }

    /// The `Digest` header value for a request body.
    #[must_use]
    pub fn body_digest(body: &[u8]) -> String {
        let digest = Sha256::digest(body);
        format!(
            "SHA-256={}",
            base64::engine::general_purpose::STANDARD.encode(digest)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_mastodon_signature_header() {
        let signature = HttpSignature::parse(
            r#"keyId="https://example.social/users/a#main-key",algorithm="rsa-sha256",headers="(request-target) host date digest",signature="abc""#,
        )
        .expect("signature");
        assert_eq!(signature.key_id, "https://example.social/users/a#main-key");
        assert_eq!(signature.algorithm, "rsa-sha256");
        assert_eq!(
            signature.headers,
            ["(request-target)", "host", "date", "digest"]
        );
        assert_eq!(signature.signature, "abc");
    }

    #[test]
    fn canonical_signing_string_keeps_header_order() {
        let signature =
            HttpSignature::parse(r#"keyId="k",headers="(request-target) host date",signature="s""#)
                .expect("signature");
        let message = signature
            .signing_string(|name| {
                Some(match name {
                    "(request-target)" => "post /inbox".to_owned(),
                    "host" => "example.social".to_owned(),
                    "date" => "Tue, 12 Nov 2026 03:00:00 GMT".to_owned(),
                    _ => return None,
                })
            })
            .expect("message");
        assert_eq!(
            message,
            "(request-target): post /inbox\nhost: example.social\ndate: Tue, 12 Nov 2026 03:00:00 GMT"
        );
    }

    #[test]
    fn display_round_trips_through_parse() {
        let header = r#"keyId="k",algorithm="rsa-sha256",headers="host date",signature="s""#;
        let signature = HttpSignature::parse(header).expect("signature");
        let parsed = HttpSignature::parse(&signature.to_string()).expect("round trip");
        assert_eq!(signature, parsed);
    }
}
